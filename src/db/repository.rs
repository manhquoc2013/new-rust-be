//! Trait repository CRUD, helper format/escape SQL, now_utc_for_db, normalize datetime.

use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::error::DbError;
use crate::db::mapper::RowMapper;
use crate::utils::now_utc_db_string;
use chrono::Local;
use odbc_api::{Cursor, Nullable};
use r2d2::Pool;
use std::fmt::Display;

/// Returns current time in UTC, format 'YYYY-MM-DD HH:MI:SS'.
/// Pool has session TIME_ZONE = 'UTC' so when saving via TO_DATE(..., 'YYYY-MM-DD HH24:MI:SS') DB interprets as UTC.
pub fn now_utc_for_db() -> String {
    now_utc_db_string()
}

/// Returns current time in local timezone (server), format 'YYYY-MM-DD HH:MI:SS'.
/// Use only when writing in machine time; prefer [`now_utc_for_db`] since DB session is UTC.
pub fn now_local_for_db() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Repository trait for CRUD operations.
pub trait Repository<T, ID>
where
    ID: Display + Clone,
{
    fn get_pool(&self) -> &Pool<OdbcConnectionManager>;
    fn table_name(&self) -> &str;
    fn primary_key(&self) -> &str;
    fn mapper(&self) -> &dyn RowMapper<T>;

    fn find_all(&self) -> Result<Vec<T>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        let query = format!("SELECT * FROM {}", self.table_name());
        let mapper = self.mapper();

        let cursor_result = conn.execute(&query, (), None)?;
        match cursor_result {
            Some(mut cursor) => {
                let mut results = Vec::new();
                loop {
                    match cursor.next_row() {
                        Ok(Some(mut row)) => match mapper.map_row(&mut row) {
                            Ok(entity) => results.push(entity),
                            Err(e) => {
                                tracing::error!(error = %e, "[DB] map_row failed");
                                continue;
                            }
                        },
                        Ok(None) => break,
                        Err(e) => {
                            return Err(DbError::ExecutionError(format!(
                                "Error fetching row: {}",
                                e
                            )))
                        }
                    }
                }
                Ok(results)
            }
            None => Ok(Vec::new()),
        }
    }

    fn find_by_id(&self, id: ID) -> Result<Option<T>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        // Escape single quotes in ID to prevent SQL injection
        let id_str = format!("{}", id);
        let escaped_id = id_str.replace("'", "''");
        let query = format!(
            "SELECT * FROM {} WHERE {} = '{}'",
            self.table_name(),
            self.primary_key(),
            escaped_id
        );
        let mapper = self.mapper();

        let cursor_result = conn.execute(&query, (), None)?;
        match cursor_result {
            Some(mut cursor) => match cursor.next_row() {
                Ok(Some(mut row)) => match mapper.map_row(&mut row) {
                    Ok(entity) => Ok(Some(entity)),
                    Err(e) => Err(e),
                },
                Ok(None) => Ok(None),
                Err(e) => Err(DbError::ExecutionError(format!(
                    "Error fetching row: {}",
                    e
                ))),
            },
            None => Ok(None),
        }
    }

    fn insert(&self, entity: &T) -> Result<ID, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        let query = self.build_insert_query(entity)?;

        let cursor_result = conn.execute(&query, (), None)?;
        match cursor_result {
            Some(_) => {
                // Get the last inserted ID
                // Note: We need to get a new connection for the query to get last ID
                // because the previous connection might be in use
                let mut conn2 = get_connection_with_retry(pool)?;
                self.get_last_insert_id(&mut conn2)
            }
            None => Err(DbError::ExecutionError(
                "Insert returned no result".to_string(),
            )),
        }
    }

    fn update(&self, id: ID, entity: &T) -> Result<bool, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        let query = self.build_update_query(id, entity)?;

        let cursor_result = conn.execute(&query, (), None)?;
        // ODBC/Altibase often return None for UPDATE (no result set); treat as success so we don't
        // incorrectly skip cache update and cause "checkin_commit_datetime/checkout not saved" when
        // the row was actually updated.
        match cursor_result {
            Some(_) => Ok(true),
            None => Ok(true),
        }
    }

    fn delete(&self, id: ID) -> Result<bool, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        // Escape single quotes in ID to prevent SQL injection
        let id_str = format!("{}", id);
        let escaped_id = id_str.replace("'", "''");
        let query = format!(
            "DELETE FROM {} WHERE {} = '{}'",
            self.table_name(),
            self.primary_key(),
            escaped_id
        );

        let cursor_result = conn.execute(&query, (), None)?;
        match cursor_result {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    fn count(&self) -> Result<i64, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        let query = format!("SELECT COUNT(*) FROM {}", self.table_name());

        let cursor_result = conn.execute(&query, (), None)?;
        match cursor_result {
            Some(mut cursor) => match cursor.next_row() {
                Ok(Some(mut row)) => {
                    let mut field = Nullable::<i64>::null();
                    row.get_data(1, &mut field).map_err(|e| {
                        DbError::ConversionError(format!("Failed to get count: {}", e))
                    })?;
                    field
                        .into_opt()
                        .ok_or_else(|| DbError::ConversionError("Count returned null".to_string()))
                }
                Ok(None) => Err(DbError::NotFound("No count result".to_string())),
                Err(e) => Err(DbError::ExecutionError(format!(
                    "Error fetching count: {}",
                    e
                ))),
            },
            None => Err(DbError::ExecutionError(
                "Count query returned no cursor".to_string(),
            )),
        }
    }

    fn build_insert_query(&self, entity: &T) -> Result<String, DbError>;
    fn build_update_query(&self, id: ID, entity: &T) -> Result<String, DbError>;
    fn get_last_insert_id(&self, _conn: &mut odbc_api::Connection<'static>) -> Result<ID, DbError> {
        Err(DbError::Other(
            "get_last_insert_id not implemented. Override this method in your repository."
                .to_string(),
        ))
    }
}

/// Escape SQL string (single quotes).
pub fn escape_sql_string(s: &str) -> String {
    s.replace("'", "''")
}

/// Format SQL value (no quoting).
pub fn format_sql_value(value: &dyn Display) -> String {
    format!("{}", value)
}

/// Format SQL string (quoted and escaped).
pub fn format_sql_string(value: &str) -> String {
    format!("'{}'", escape_sql_string(value))
}

/// Format Option<String> to SQL (NULL if None).
pub fn format_sql_nullable_string(value: &Option<String>) -> String {
    match value {
        Some(s) => format_sql_string(s),
        None => "NULL".to_string(),
    }
}

/// Format Option<i64> to SQL (NULL if None).
pub fn format_sql_nullable_i64(value: &Option<i64>) -> String {
    match value {
        Some(v) => format_sql_value(v),
        None => "NULL".to_string(),
    }
}

/// Format Option<i32> to SQL (NULL if None).
pub fn format_sql_nullable_i32(value: &Option<i32>) -> String {
    match value {
        Some(v) => format_sql_value(v),
        None => "NULL".to_string(),
    }
}

/// Format Option<f64> to SQL (NULL if None).
pub fn format_sql_nullable_f64(value: &Option<f64>) -> String {
    match value {
        Some(v) => format_sql_value(v),
        None => "NULL".to_string(),
    }
}

/// Normalize datetime string from DB (ODBC/Altibase) to 'YYYY-MM-DD HH24:MI:SS'. Supports YYYY-MM-DD, DD-MON-YYYY, with/without time.
pub fn normalize_datetime_string(dt: &str) -> String {
    let dt = dt.trim();

    // YYYY-MM-DD format (with optional time)
    if dt.len() >= 10 && dt.chars().nth(4) == Some('-') && dt.chars().nth(7) == Some('-') {
        let date_part = &dt[..10];
        if dt.len() == 10 {
            // Date only → add 00:00:00 so TO_DATE always has time part (avoid losing time on read/write)
            return format!("{} 00:00:00", date_part);
        }
        // Has time part: take "YYYY-MM-DD HH:MM:SS", drop .fff if present
        let rest = dt[10..].trim_start();
        let time_part = if let Some(dot) = rest.find('.') {
            rest[..dot].trim_end()
        } else {
            rest
        };
        if time_part.len() >= 8 {
            return format!("{} {}", date_part, time_part);
        }
        return format!("{} 00:00:00", date_part);
    }

    // Try to parse DD-MON-YYYY format
    let parts: Vec<&str> = dt.split(|c| c == '-' || c == ' ').collect();
    if parts.len() >= 3 {
        let day = parts[0];
        let month_str = parts[1].to_uppercase();
        let year = parts[2];

        // Convert month name to number
        let month = match month_str.as_str() {
            "JAN" => "01",
            "FEB" => "02",
            "MAR" => "03",
            "APR" => "04",
            "MAY" => "05",
            "JUN" => "06",
            "JUL" => "07",
            "AUG" => "08",
            "SEP" => "09",
            "OCT" => "10",
            "NOV" => "11",
            "DEC" => "12",
            _ => return dt.to_string(), // Can't parse, return original
        };

        // Get time part if exists
        let time = if parts.len() >= 4 {
            parts[3..].join(":")
        } else {
            "00:00:00".to_string()
        };

        // Ensure time has correct format
        let time = if time.len() >= 8 {
            time
        } else {
            format!("{}:00:00", time)
        };

        return format!("{}-{}-{} {}", year, month, day, time);
    }

    // Can't parse, return original
    dt.to_string()
}

/// Normalize datetime string then wrap in TO_DATE. Use when building SQL with &str (e.g. partial UPDATE).
/// Empty/whitespace string is invalid for TO_DATE → return safe literal (caller should use NULL for nullable).
pub fn format_sql_datetime_str(dt: &str) -> String {
    let dt = dt.trim();
    if dt.is_empty() {
        // Avoid TO_DATE('', ...) causing ODBC error; caller nullable should use format_sql_nullable_datetime for NULL
        return "NULL".to_string();
    }
    let normalized = normalize_datetime_string(dt);
    if normalized.is_empty() {
        return "NULL".to_string();
    }
    format!(
        "TO_DATE('{}', 'YYYY-MM-DD HH24:MI:SS')",
        escape_sql_string(&normalized)
    )
}

/// Format Option<String> datetime to TO_DATE(...) (NULL if None). DB column should use TIMESTAMP to preserve time.
/// Some("") or whitespace-only string treated as NULL to avoid ODBC error on UPDATE.
pub fn format_sql_nullable_datetime(value: &Option<String>) -> String {
    match value {
        Some(dt) if !dt.trim().is_empty() => format_sql_datetime_str(dt),
        _ => "NULL".to_string(),
    }
}
