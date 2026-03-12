//! ODBC connection pool (OdbcConnectionManager), create pool from env (DSN, connection string).
//! Reconnect: test_on_check_out + get_connection_with_retry when connection is flaky or lost.

use dotenvy::dotenv;
use odbc_api::{Connection, ConnectionOptions, Environment, Error};
use once_cell::sync::Lazy;
use r2d2::{ManageConnection, Pool, PooledConnection};
use std::{env, thread, time::Duration};

use crate::constants::pool;

static ODBC_ENV: Lazy<Environment> =
    Lazy::new(|| Environment::new().expect("Failed to create ODBC environment"));

#[derive(Debug)]
pub struct OdbcConnectionManager {
    conn_str: String,
}

impl OdbcConnectionManager {
    pub fn new(conn_str: String) -> Self {
        Self { conn_str }
    }
}

impl ManageConnection for OdbcConnectionManager {
    type Connection = Connection<'static>;
    type Error = Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let env: &'static Environment = &ODBC_ENV;
        let conn =
            env.connect_with_connection_string(&self.conn_str, ConnectionOptions::default())?;
        // Oracle/Altibase: set NLS so DATE/TIMESTAMP read as text include time. Best-effort:
        // Altibase ODBC may return HY000 266371 for unsupported properties; skip logging that case to avoid spam.
        let nls_stmts = [
            "ALTER SESSION SET NLS_DATE_FORMAT = 'YYYY-MM-DD HH24:MI:SS'",
            "ALTER SESSION SET NLS_TIMESTAMP_FORMAT = 'YYYY-MM-DD HH24:MI:SS'",
            "ALTER SESSION SET TIME_ZONE = 'UTC'",
        ];
        for stmt in nls_stmts {
            if let Err(e) = conn.execute(stmt, (), None) {
                let err_str = e.to_string();
                let is_unsupported_property = err_str.contains("266371")
                    || err_str.contains("property does not exist")
                    || err_str.contains("cannot be updated");
                if !is_unsupported_property {
                    tracing::debug!(stmt = %stmt, error = %e, "[DB] ODBC NLS/session setup skipped");
                }
            }
        }
        Ok(conn)
    }

    /// Check connection still valid; r2d2 calls when getting connection from pool.
    fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        conn.execute("SELECT 1", (), None)?;
        Ok(())
    }

    /// Detect broken connection; pool will close and create new (called when returning connection to pool).
    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        match conn.execute("SELECT 1", (), None) {
            Ok(_) => false,
            Err(e) => {
                tracing::debug!(
                    error = %e,
                    "[DB] ODBC connection broken, will be replaced"
                );
                true
            }
        }
    }
}

/// Get connection from pool with retry + exponential backoff when DB is lost/flaky.
///
/// **Performance**: When pool is healthy, only call `pool.get()` once (same as direct call) → no extra latency.
/// Only when `pool.get()` fails do we retry and sleep; all runs in blocking context (spawn_blocking) so it does not block async runtime.
pub fn get_connection_with_retry(
    pool: &Pool<OdbcConnectionManager>,
) -> Result<PooledConnection<OdbcConnectionManager>, r2d2::Error> {
    let mut backoff_ms = pool::GET_CONNECTION_INITIAL_BACKOFF_MS;
    for attempt in 1..=pool::GET_CONNECTION_MAX_RETRIES {
        match pool.get() {
            Ok(conn) => return Ok(conn),
            Err(e) => {
                if attempt < pool::GET_CONNECTION_MAX_RETRIES {
                    tracing::debug!(
                        attempt,
                        max_retries = pool::GET_CONNECTION_MAX_RETRIES,
                        backoff_ms,
                        error = %e,
                        "[DB] pool get failed, retrying"
                    );
                    thread::sleep(Duration::from_millis(backoff_ms));
                    backoff_ms = (backoff_ms * 2).min(pool::GET_CONNECTION_MAX_BACKOFF_MS);
                } else {
                    tracing::error!(
                        attempt,
                        error = %e,
                        "[DB] pool get failed after retries"
                    );
                    return Err(e);
                }
            }
        }
    }
    unreachable!()
}

pub fn create_pool(prefix: &str) -> Pool<OdbcConnectionManager> {
    dotenv().ok();

    let get = |field: &str| {
        let key = format!("{}_{}", prefix, field);
        env::var(&key).unwrap_or_else(|_| panic!("Missing {}", key))
    };

    let get_opt = |field: &str| {
        let key = format!("{}_{}", prefix, field);
        env::var(&key).ok().map(|s| s.trim().to_string())
    };

    let user = get("DB_USER").trim().to_string();
    let pass = get("DB_PASS").trim().to_string();

    // Prefer DSN if set: avoids "Connection string pair [Driver=...] ignored" (01S00) and
    // "Driver does not support the requested version" (01000) warnings with Altibase/unixODBC.
    // DSN-based connections handle ODBC version negotiation better than direct Driver connections.
    let (conn_str, masked_conn_str, connect_target) = if let Some(dsn) = get_opt("DB_DSN") {
        let c = format!("DSN={};UID={};PWD={};", dsn, user, pass);
        let m = format!("DSN={};UID={};PWD=***;", dsn, user);
        (c, m, format!("DSN:{}", dsn))
    } else {
        tracing::info!(prefix = %prefix, "[DB] Using Driver connection; set prefix_DB_DSN for DSN");
        let driver = get("DB_DRIVER").trim().to_string();
        let host = get("DB_HOST").trim().to_string();
        let port = get("DB_PORT").trim().to_string();
        // Standard Driver=name connection string. With Altibase/unixODBC you may see 01S00/01000/HY000
        // warnings; they are usually harmless. For fewer warnings, use DSN (e.g. RATING_DB_DSN).
        let c = format!(
            "Driver={};Server={};Port={};UID={};PWD={};",
            driver, host, port, user, pass
        );
        let m = format!(
            "Driver={};Server={};Port={};UID={};PWD=***;",
            driver, host, port, user
        );
        (c, m, format!("{}:{}", host, port))
    };

    let max_size: u32 = get("DB_POOL_SIZE").parse().expect("Invalid POOL_SIZE");

    let min_idle = env::var(format!("{}_DB_MIN_IDLE", prefix))
        .ok()
        .map(|v| v.parse().expect("Invalid MIN_IDLE"));

    let timeout_seconds: u64 = env::var(format!("{}_DB_TIMEOUT_SECONDS", prefix))
        .unwrap_or_else(|_| "5".into())
        .parse()
        .expect("Invalid TIMEOUT_SECONDS");

    // Optimization: Add idle_timeout to auto-close idle connections (default: 10 min)
    // Under high throughput, connections may be closed by DB server if idle too long
    let idle_timeout_seconds: u64 = env::var(format!("{}_DB_IDLE_TIMEOUT_SECONDS", prefix))
        .unwrap_or_else(|_| "600".into()) // Default 10 min
        .parse()
        .expect("Invalid IDLE_TIMEOUT_SECONDS");

    // Optimization: Add max_lifetime to auto-close old connections (default: 30 min)
    // Avoid using connections that are too old and may be broken
    let max_lifetime_seconds: u64 = env::var(format!("{}_DB_MAX_LIFETIME_SECONDS", prefix))
        .unwrap_or_else(|_| "1800".into()) // Default 30 min
        .parse()
        .expect("Invalid MAX_LIFETIME_SECONDS");

    let manager = OdbcConnectionManager::new(conn_str.clone());

    tracing::debug!(prefix = %prefix, conn = %masked_conn_str, "[DB] connection string");
    tracing::info!(prefix = %prefix, max_size, min_idle = ?min_idle, timeout_secs = timeout_seconds, idle_timeout_secs = idle_timeout_seconds, max_lifetime_secs = max_lifetime_seconds, "[DB] pool config");
    tracing::debug!(prefix = %prefix, "[DB] validation: is_valid/has_broken");

    // test_on_check_out: true (also r2d2 default) → on each checkout call is_valid() (SELECT 1), avoid handing out dead connection.
    // No extra cost vs default; needed when connection is flaky.
    let pool = match Pool::builder()
        .max_size(max_size)
        .min_idle(min_idle)
        .connection_timeout(Duration::from_secs(timeout_seconds))
        .idle_timeout(Some(Duration::from_secs(idle_timeout_seconds)))
        .max_lifetime(Some(Duration::from_secs(max_lifetime_seconds)))
        .test_on_check_out(true)
        .build(manager)
    {
        Ok(p) => {
            // Test the connection to ensure it actually works despite any warnings
            match p.get() {
                Ok(conn) => {
                    // Test with a simple query to verify the connection works
                    match conn.execute("SELECT 1", (), None) {
                        Ok(_) => {
                            tracing::info!(prefix = %prefix, target = %connect_target, "[DB] pool OK");
                        }
                        Err(e) => {
                            tracing::error!(prefix = %prefix, error = %e, "[DB] pool test query failed");
                            tracing::error!(prefix = %prefix, conn = %masked_conn_str, "[DB] tip: set DSN");
                            panic!("Cannot execute test query for {}: {}", prefix, e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(prefix = %prefix, error = %e, "[DB] pool test connection failed");
                    tracing::error!(prefix = %prefix, conn = %masked_conn_str, "[DB] tip: set DSN");
                    panic!("Cannot establish database connection for {}: {}", prefix, e);
                }
            }
            p
        }
        Err(e) => {
            tracing::error!(prefix = %prefix, error = %e, "[DB] failed to create ODBC pool");
            tracing::error!(prefix = %prefix, conn = %masked_conn_str, "[DB] tip: set DSN");
            panic!("Cannot create ODBC pool for {}: {}", prefix, e);
        }
    };
    std::io::Write::flush(&mut std::io::stdout()).ok();

    pool
}
