//! ETC Transaction Middleware entry point: TCP server, logic handler, cache, logging.

#![allow(clippy::too_many_arguments)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::unwrap_or_default)]
#![allow(clippy::single_match)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::redundant_locals)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_borrow)]

mod cache;
mod configs;
mod constants;
mod crypto;
mod db;
mod fe_protocol;
mod handlers;
mod logging;
mod logic;
mod models;
mod network;
mod price_ticket_type;
mod services;
mod types;
mod utils;

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::config::keydb::KeyDB;
use crate::cache::config::moka_cache::MokaCache;
use crate::cache::data::cache_reload::start_cache_reload_task;
use crate::cache::data::closed_cycle_transition_stage_cache::get_closed_cycle_transition_stage_cache;
use crate::cache::data::connection_data_cache::{
    get_connection_server_cache, get_connection_user_cache,
};
use crate::cache::data::db_retry::{run_db_retry_task, set_keydb as set_db_retry_keydb};
use crate::cache::data::price_cache::get_price_cache;
use crate::cache::data::subscription_history_cache::get_subscription_history_cache;
use crate::cache::data::toll_cache::get_toll_cache;
use crate::cache::data::toll_fee_list_cache::get_toll_fee_list_cache;
use crate::cache::data::toll_lane_cache::get_toll_lane_cache;
use crate::configs::config::*;
use crate::configs::crm_db::CRM_DB;
use crate::configs::mediation_db::MEDIATION_DB;
use crate::configs::pool_factory::create_pool;
use crate::configs::rating_db::RATING_DB;
use crate::logging::{
    get_logging_config_from_env, init_logging, set_process_type, start_log_cleanup_task,
    CleanupConfig, ProcessType,
};
use crate::logic::run_request_processor;
use crate::models::ETDR::{
    flush_unsaved_etdrs_to_keydb, set_keydb_for_etdr_sync, start_cache_cleanup_task,
    start_etdr_db_retry_task,
};
use crate::network::{run_connection_router, run_tcp_server};
use crate::types::{ConnectionId, ConnectionMap, RequestToProcess};
use once_cell::sync::Lazy;
use std::error::Error;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Wait for shutdown signal: Ctrl+C (SIGINT) or SIGTERM (e.g. K8s pod terminate).
#[cfg(unix)]
async fn wait_for_shutdown_signal() -> Result<(), Box<dyn Error>> {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm =
        signal(SignalKind::terminate()).map_err(|e| format!("SIGTERM stream: {}", e))?;
    tokio::select! {
        res = tokio::signal::ctrl_c() => res.map_err(|e| e.into()),
        _ = sigterm.recv() => {
            tracing::info!("[STARTUP] SIGTERM received (e.g. K8s pod terminate)");
            Ok(())
        }
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown_signal() -> Result<(), Box<dyn Error>> {
    tokio::signal::ctrl_c().await.map_err(|e| e.into())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging before any other operations
    let log_config = get_logging_config_from_env();
    init_logging(log_config.clone()).map_err(|e| {
        eprintln!("[STARTUP] Failed to initialize logging: {}", e);
        e
    })?;

    let cleanup_config = CleanupConfig {
        log_dir: log_config.log_dir.clone(),
        log_file_pattern: log_config.log_file_name.clone(),
        retention_days: log_config.retention_days,
        cleanup_interval_seconds: 3600,
        statistics_retention_days: Some(30),
    };
    start_log_cleanup_task(cleanup_config);

    std::panic::set_hook(Box::new(|panic_info| {
        tracing::error!(panic = ?panic_info, "[STARTUP] panic");
        eprintln!("[STARTUP] PANIC: {:?}", panic_info);
        std::io::stderr().flush().ok();
        std::io::stdout().flush().ok();
    }));

    let cfg = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "[STARTUP] config load failed");
            std::process::exit(1);
        }
    };
    tracing::info!(port = cfg.port_listen, "[STARTUP] config loaded, listening");

    let db_init_result = std::panic::catch_unwind(|| {
        Lazy::force(&RATING_DB);
        Lazy::force(&MEDIATION_DB);
        Lazy::force(&CRM_DB);
    });
    let rating_pool_holder: Arc<
        std::sync::RwLock<
            Option<Arc<r2d2::Pool<crate::configs::pool_factory::OdbcConnectionManager>>>,
        >,
    > = Arc::new(std::sync::RwLock::new(None));
    let mediation_holder: Arc<
        std::sync::RwLock<
            Option<Arc<r2d2::Pool<crate::configs::pool_factory::OdbcConnectionManager>>>,
        >,
    > = Arc::new(std::sync::RwLock::new(None));
    let crm_pool_holder: Arc<
        std::sync::RwLock<
            Option<Arc<r2d2::Pool<crate::configs::pool_factory::OdbcConnectionManager>>>,
        >,
    > = Arc::new(std::sync::RwLock::new(None));
    if db_init_result.is_ok() {
        if let Ok(mut g) = rating_pool_holder.write() {
            *g = Some(RATING_DB.clone());
        }
        if let Ok(mut guard) = mediation_holder.write() {
            *guard = Some(Arc::new(MEDIATION_DB.clone()));
        }
        if let Ok(mut guard) = crm_pool_holder.write() {
            *guard = Some(Arc::new(CRM_DB.clone()));
        }
        tracing::info!("[STARTUP] database pools ready (RATING, MEDIATION, CRM)");
    } else {
        tracing::warn!("[STARTUP] database init failed; app continues, RATING/MEDIATION/CRM cache will use KeyDB fallback or retry in background");
        let cache_delay_secs = std::env::var("CACHE_LOAD_DELAY_AFTER_DB_FAIL_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(5);
        tracing::info!(
            delay_secs = cache_delay_secs,
            "[STARTUP] waiting before cache load (DB init failed)"
        );
        tokio::time::sleep(Duration::from_secs(cache_delay_secs)).await;
    }

    let moka = Arc::new(MokaCache::new());
    let keydb_url =
        std::env::var("KEYDB_URL").unwrap_or("redis://default@120.0.0.1:30222".to_string());
    let keydb = KeyDB::new(&keydb_url).await;
    let cache = Arc::new(CacheManager::new(moka, keydb));

    let keydb_connect_timeout_ms = std::env::var("KEYDB_CONNECT_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(2000);
    let keydb_connected = tokio::time::timeout(
        std::time::Duration::from_millis(keydb_connect_timeout_ms),
        async {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
            interval.tick().await;
            while !cache.keydb().is_connected().await {
                interval.tick().await;
            }
        },
    )
    .await
    .is_ok();

    let load_from_keydb = keydb_connected;
    if !keydb_connected {
        tracing::info!(
            timeout_ms = keydb_connect_timeout_ms,
            "[STARTUP] KeyDB not available, cache load from DB only (no fallback); ETDR and ticket_id use fallback"
        );
    } else {
        set_keydb_for_etdr_sync(cache.keydb());
        tracing::info!(
            "[STARTUP] Cache load: DB first, KeyDB fallback on DB failure; ETDR sync to KeyDB enabled (key prefix: etdr:)"
        );
        tracing::info!("[STARTUP] ticket_id generator: local (UUID + hash, MSB=1)");
    }

    let rating_pool = rating_pool_holder.read().ok().and_then(|g| g.clone());
    let t_start = std::time::Instant::now();
    let ((), (), (), (), (), ()) = tokio::join!(
        get_toll_cache(rating_pool.clone(), cache.clone(), load_from_keydb),
        get_toll_lane_cache(rating_pool.clone(), cache.clone(), load_from_keydb),
        get_price_cache(rating_pool.clone(), cache.clone(), load_from_keydb),
        get_closed_cycle_transition_stage_cache(
            rating_pool.clone(),
            cache.clone(),
            load_from_keydb
        ),
        get_toll_fee_list_cache(rating_pool.clone(), cache.clone(), load_from_keydb),
        get_subscription_history_cache(rating_pool, cache.clone(), load_from_keydb),
    );
    tracing::info!(elapsed = ?t_start.elapsed(), "[STARTUP] cache loaded (toll, lane, price, closed_cycle_transition_stage, toll_fee_list, subscription_history)");

    let cache_reload_interval = std::env::var("CACHE_RELOAD_INTERVAL_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1800);
    start_cache_reload_task(
        rating_pool_holder.clone(),
        cache.clone(),
        cache_reload_interval,
        mediation_holder.clone(),
    );
    // Connection cache load after reload task: first reload runs after 1 interval so no overlap; connection only reloads when mediation_holder has pool.
    let mediation_pool = mediation_holder.read().ok().and_then(|g| g.clone());
    tokio::join!(
        get_connection_server_cache(mediation_pool.clone(), cache.clone(), load_from_keydb),
        get_connection_user_cache(mediation_pool, cache.clone(), load_from_keydb),
    );
    tracing::info!("[STARTUP] connection data cache loaded (server, user)");

    set_db_retry_keydb(cache.keydb());
    let db_retry_interval = std::env::var("DB_RETRY_INTERVAL_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10);
    let mediation_holder_db_retry = mediation_holder.clone();
    tokio::spawn(async move {
        run_db_retry_task(db_retry_interval, mediation_holder_db_retry).await;
    });
    tracing::info!(
        interval_secs = db_retry_interval,
        "[STARTUP] DB retry task started"
    );

    // Reconnect MEDIATION: when DB init fails holder = None; periodic task tries create_pool("MEDIATION").
    // On success: clear holder + connection cache then set new pool and load from DB (same as startup).
    let db_init_retry_interval = std::env::var("DB_INIT_RETRY_INTERVAL_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60);
    let mediation_holder_retry = mediation_holder.clone();
    let cache_for_mediation_retry = cache.clone();
    tokio::spawn(async move {
        let mut interval_timer = tokio::time::interval(Duration::from_secs(db_init_retry_interval));
        interval_timer.tick().await;
        loop {
            interval_timer.tick().await;
            let has_pool = mediation_holder_retry
                .read()
                .ok()
                .map(|g| g.is_some())
                .unwrap_or(false);
            if has_pool {
                continue;
            }
            let result = tokio::task::spawn_blocking(|| {
                std::panic::catch_unwind(|| create_pool("MEDIATION"))
            })
            .await;
            match result {
                Ok(Ok(pool)) => {
                    let pool = Arc::new(pool);
                    if let Ok(mut g) = mediation_holder_retry.write() {
                        *g = Some(pool.clone());
                    }
                    let p = pool.clone();
                    tokio::join!(
                        get_connection_server_cache(
                            Some(pool),
                            cache_for_mediation_retry.clone(),
                            false
                        ),
                        get_connection_user_cache(
                            Some(p),
                            cache_for_mediation_retry.clone(),
                            false
                        ),
                    );
                    tracing::info!("[DB] MEDIATION reconnected: pool set, connection cache loaded from DB (cache data kept until load)");
                }
                Ok(Err(_)) => {
                    tracing::debug!("[DB] MEDIATION pool retry failed (will retry)");
                }
                Err(e) => {
                    tracing::debug!(error = %e, "[DB] MEDIATION pool retry task join error");
                }
            }
        }
    });
    tracing::info!(
        interval_secs = db_init_retry_interval,
        "[STARTUP] DB init retry task started (MEDIATION)"
    );

    // Reconnect RATING: when DB init fails holder = None; periodic task tries create_pool("RATING"). On success: set new pool and load from DB (keep cache until load overwrites).
    let rating_holder_retry = rating_pool_holder.clone();
    let cache_for_rating_retry = cache.clone();
    tokio::spawn(async move {
        let mut interval_timer = tokio::time::interval(Duration::from_secs(db_init_retry_interval));
        interval_timer.tick().await;
        loop {
            interval_timer.tick().await;
            let has_pool = rating_holder_retry
                .read()
                .ok()
                .map(|g| g.is_some())
                .unwrap_or(false);
            if has_pool {
                continue;
            }
            let result =
                tokio::task::spawn_blocking(|| std::panic::catch_unwind(|| create_pool("RATING")))
                    .await;
            match result {
                Ok(Ok(pool)) => {
                    let pool = Arc::new(pool);
                    if let Ok(mut g) = rating_holder_retry.write() {
                        *g = Some(pool.clone());
                    }
                    let load_from_keydb = false;
                    let rating_pool = Some(pool.clone());
                    let ((), (), (), (), (), ()) = tokio::join!(
                        get_toll_cache(
                            rating_pool.clone(),
                            cache_for_rating_retry.clone(),
                            load_from_keydb
                        ),
                        get_toll_lane_cache(
                            rating_pool.clone(),
                            cache_for_rating_retry.clone(),
                            load_from_keydb
                        ),
                        get_price_cache(
                            rating_pool.clone(),
                            cache_for_rating_retry.clone(),
                            load_from_keydb
                        ),
                        get_closed_cycle_transition_stage_cache(
                            rating_pool.clone(),
                            cache_for_rating_retry.clone(),
                            load_from_keydb
                        ),
                        get_toll_fee_list_cache(
                            rating_pool.clone(),
                            cache_for_rating_retry.clone(),
                            load_from_keydb
                        ),
                        get_subscription_history_cache(
                            rating_pool,
                            cache_for_rating_retry.clone(),
                            load_from_keydb
                        ),
                    );
                    tracing::info!("[DB] RATING reconnected: pool set, caches loaded from DB (cache data kept until load)");
                }
                Ok(Err(_)) => {
                    tracing::debug!("[DB] RATING pool retry failed (will retry)");
                }
                Err(e) => {
                    tracing::debug!(error = %e, "[DB] RATING pool retry task join error");
                }
            }
        }
    });
    tracing::info!(
        interval_secs = db_init_retry_interval,
        "[STARTUP] DB init retry task started (RATING)"
    );

    // Reconnect CRM: when DB init fails holder = None; periodic task tries create_pool("CRM"). On success: clear holder then set new pool (same as startup).
    let crm_holder_retry = crm_pool_holder.clone();
    tokio::spawn(async move {
        let mut interval_timer = tokio::time::interval(Duration::from_secs(db_init_retry_interval));
        interval_timer.tick().await;
        loop {
            interval_timer.tick().await;
            let has_pool = crm_holder_retry
                .read()
                .ok()
                .map(|g| g.is_some())
                .unwrap_or(false);
            if has_pool {
                continue;
            }
            let result =
                tokio::task::spawn_blocking(|| std::panic::catch_unwind(|| create_pool("CRM")))
                    .await;
            match result {
                Ok(Ok(pool)) => {
                    let pool = Arc::new(pool);
                    if let Ok(mut g) = crm_holder_retry.write() {
                        *g = None;
                    }
                    if let Ok(mut g) = crm_holder_retry.write() {
                        *g = Some(pool);
                    }
                    tracing::info!("[DB] CRM reconnected: pool reinitialized (clear + set)");
                }
                Ok(Err(_)) => {
                    tracing::debug!("[DB] CRM pool retry failed (will retry)");
                }
                Err(e) => {
                    tracing::debug!(error = %e, "[DB] CRM pool retry task join error");
                }
            }
        }
    });
    tracing::info!(
        interval_secs = db_init_retry_interval,
        "[STARTUP] DB init retry task started (CRM)"
    );

    let etdr_cleanup_interval = std::env::var("ETDR_CLEANUP_INTERVAL_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(3600);
    start_cache_cleanup_task(etdr_cleanup_interval);

    let etdr_retry_interval = std::env::var("ETDR_RETRY_INTERVAL_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(30);
    let etdr_max_retry_count = std::env::var("ETDR_MAX_RETRY_COUNT")
        .ok()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(10);
    start_etdr_db_retry_task(etdr_retry_interval, etdr_max_retry_count);
    tracing::info!(
        interval_secs = etdr_retry_interval,
        max_retry = etdr_max_retry_count,
        "[STARTUP] ETDR retry task started"
    );

    let reply_in_flight_limit = crate::configs::config::Config::get().reply_in_flight_limit;
    let channel_cap = if reply_in_flight_limit > 0 {
        reply_in_flight_limit
    } else {
        256
    };

    let (tx_request_to_process, rx_request_to_process): (
        mpsc::Sender<RequestToProcess>,
        mpsc::Receiver<RequestToProcess>,
    ) = mpsc::channel(channel_cap);

    let (tx_logic_replies, rx_logic_replies): (
        mpsc::Sender<crate::types::ReplyToRoute>,
        mpsc::Receiver<crate::types::ReplyToRoute>,
    ) = mpsc::channel(channel_cap);

    let reply_in_flight = if reply_in_flight_limit > 0 {
        Some(Arc::new(tokio::sync::Semaphore::new(reply_in_flight_limit)))
    } else {
        None
    };

    let (tx_session_updates, rx_session_updates) =
        tokio::sync::mpsc::unbounded_channel::<crate::types::SessionUpdate>();
    let (tx_conn_closed, rx_conn_closed) = tokio::sync::mpsc::unbounded_channel::<ConnectionId>();

    let active_conns: ConnectionMap =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

    let server_active_conns = active_conns.clone();
    let server_tx_requests = tx_request_to_process.clone();
    let server_tx_replies = tx_logic_replies.clone();
    let server_cache = cache.clone();
    let server_tx_session = tx_session_updates.clone();
    let server_rx_session = rx_session_updates;

    tokio::spawn(async move {
        set_process_type(ProcessType::Local);
        if let Err(e) = run_tcp_server(
            server_active_conns,
            server_tx_requests,
            server_tx_replies,
            server_cache,
            tx_conn_closed,
            server_tx_session,
            server_rx_session,
        )
        .await
        {
            tracing::error!(error = %e, "[STARTUP] server error");
        }
    });

    let router_active_conns = active_conns.clone();
    tokio::spawn(async move {
        set_process_type(ProcessType::Local);
        run_connection_router(rx_logic_replies, rx_conn_closed, router_active_conns).await;
    });

    let processor_cache = cache.clone();
    tokio::spawn(async move {
        set_process_type(ProcessType::Local);
        run_request_processor(
            rx_request_to_process,
            tx_logic_replies,
            processor_cache,
            tx_session_updates,
            reply_in_flight,
        )
        .await;
    });

    tracing::info!("[STARTUP] server running (Ctrl+C or SIGTERM to stop)");
    match wait_for_shutdown_signal().await {
        Ok(()) => {
            tracing::info!("[STARTUP] shutdown signal received, graceful shutdown");
            tokio::time::sleep(Duration::from_millis(500)).await;

            let flushed = flush_unsaved_etdrs_to_keydb().await;
            if flushed > 0 {
                tracing::info!(
                    count = flushed,
                    "[STARTUP] flushed unsaved ETDRs to KeyDB for retry after restart"
                );
            }

            let remaining_conns = {
                let conns = active_conns.lock().unwrap();
                conns.len()
            };
            if remaining_conns > 0 {
                tracing::info!(remaining = remaining_conns, "[STARTUP] closing connections");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;

            tracing::info!("[STARTUP] shutdown complete");
        }
        Err(e) => {
            tracing::warn!(error = %e, "[STARTUP] unable to listen for shutdown signal");
            tokio::time::sleep(Duration::from_secs(3600 * 24)).await;
        }
    }

    Ok(())
}
