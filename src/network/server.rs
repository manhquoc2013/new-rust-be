//! TCP server: listen for connections, handshake, route message to logic, TERMINATE_RESP on close.

use crate::cache::config::cache_manager::CacheManager;
use crate::configs::config::Config;
use crate::services::ip_block_service::IpBlockService;
use crate::types::{
    ConnectionId, ConnectionMap, EncryptionKeyMap, IncomingMessage,
    SessionMap, SessionUpdate,
};
use crate::utils::CompiledIpDenylist;
use std::error::Error;
use std::sync::{
    atomic::{AtomicBool, AtomicI32, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{Sender, UnboundedSender};
use tokio::sync::Semaphore;

use super::client_ip::get_real_client_ip;
use super::connection::handle_connection;
use super::session::{spawn_session_idle_cleanup, spawn_session_update_processor};
use super::terminate;

/// Chạy TCP server, chấp nhận nhiều kết nối.
pub async fn run_tcp_server(
    active_conns: ConnectionMap,
    tx_client_requests: Sender<IncomingMessage>,
    cache: Arc<CacheManager>,
    reply_in_flight: Option<Arc<Semaphore>>,
    tx_conn_closed: UnboundedSender<ConnectionId>,
) -> Result<(), Box<dyn Error>> {
    let cfg = Config::get();
    let listener = TcpListener::bind(format!("0.0.0.0:{}", cfg.port_listen)).await?;
    tracing::info!(port = cfg.port_listen, "[Network] TCP server listening");

    let session_map: SessionMap =
        Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
    let (tx_session_updates, rx_session_updates) = tokio::sync::mpsc::unbounded_channel();
    spawn_session_update_processor(rx_session_updates, session_map.clone());

    type CloseSignalMap =
        Arc<Mutex<std::collections::HashMap<ConnectionId, tokio::sync::oneshot::Sender<()>>>>;
    let close_signals: CloseSignalMap = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let encryption_keys: EncryptionKeyMap = Arc::new(Mutex::new(std::collections::HashMap::new()));

    let cfg_timeout = Config::get();
    let idle_timeout = Duration::from_secs(cfg_timeout.connection_idle_timeout_seconds);
    let check_interval = Duration::from_secs(cfg_timeout.dead_connection_check_interval_seconds);
    spawn_session_idle_cleanup(
        session_map.clone(),
        active_conns.clone(),
        tx_session_updates.clone(),
        idle_timeout,
        check_interval,
    );

    static NEXT_CONN_ID: AtomicI32 = AtomicI32::new(1);
    let handshake_timeout = Duration::from_secs(cfg.handshake_timeout_seconds);
    let active_conns_count = Arc::new(AtomicI32::new(0));

    let ip_block_service: Arc<IpBlockService> = Arc::new(IpBlockService::new(cache.keydb()));
    let ip_block_enabled = cfg.ip_block_enabled;
    let ip_block_threshold = cfg.ip_block_failure_threshold;
    let ip_block_duration_secs = cfg.ip_block_duration_seconds;
    let ip_denylist = CompiledIpDenylist::compile(&cfg.ip_denylist);

    loop {
        let (socket, addr) = listener.accept().await?;
        let client_ip = get_real_client_ip(&socket, addr);

        if !ip_denylist.is_empty() && ip_denylist.matches(client_ip) {
            drop(socket);
            continue;
        }

        if ip_block_enabled && ip_block_threshold > 0 {
            if ip_block_service
                .is_blocked(client_ip, ip_block_duration_secs)
                .await
            {
                tracing::warn!(
                    ip = %client_ip,
                    "[Network] rejecting connection from blocked IP"
                );
                drop(socket);
                continue;
            }
        }

        active_conns_count.fetch_add(1, Ordering::Relaxed);

        let conn_id = NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed);
        if conn_id == i32::MAX {
            NEXT_CONN_ID.store(1, Ordering::Relaxed);
        }

        tracing::info!(conn_id, addr = %addr, total = active_conns_count.load(Ordering::Relaxed), "[Network] new connection");

        let (tx_socket_write, rx_socket_write) = tokio::sync::mpsc::channel(64);
        {
            let mut conns = active_conns.lock().unwrap();
            conns.insert(conn_id, tx_socket_write);
        }

        let tx_requests_clone = tx_client_requests.clone();
        let active_conns_clone = active_conns.clone();
        let cache_clone = cache.clone();
        let tx_session_updates_clone = tx_session_updates.clone();
        let close_signals_for_conn = close_signals.clone();
        let encryption_keys_for_conn = encryption_keys.clone();
        let conn_count_for_cleanup = active_conns_count.clone();
        let handshake_timeout_for_conn = handshake_timeout;
        let session_map_for_handshake = session_map.clone();
        let peer_disconnected = Arc::new(AtomicBool::new(false));
        let reply_in_flight_for_conn = reply_in_flight.clone();
        let tx_conn_closed_for_conn = tx_conn_closed.clone();

        let addr_clone = addr;
        let client_ip_for_block = client_ip;
        let ip_block_service_clone = ip_block_service.clone();

        tokio::spawn(async move {
            struct ConnectionCountGuard {
                conn_id: ConnectionId,
                conn_count: Arc<AtomicI32>,
            }
            impl Drop for ConnectionCountGuard {
                fn drop(&mut self) {
                    let remaining = self.conn_count.fetch_sub(1, Ordering::Relaxed) - 1;
                    tracing::debug!(
                        conn_id = self.conn_id,
                        remaining,
                        "[Network] ConnectionCountGuard decremented"
                    );
                }
            }
            let _conn_count_guard = ConnectionCountGuard {
                conn_id,
                conn_count: conn_count_for_cleanup.clone(),
            };

            let (tx_close_signal, rx_close_signal) = tokio::sync::oneshot::channel();
            {
                let mut signals = close_signals_for_conn.lock().unwrap();
                signals.insert(conn_id, tx_close_signal);
            }

            let handshake_timeout_handle = if handshake_timeout_for_conn.as_secs() > 0 {
                let close_signals_for_timeout = close_signals_for_conn.clone();
                let session_map_for_timeout = session_map_for_handshake.clone();
                let conn_id_for_timeout = conn_id;
                let tx_session_updates_for_timeout = tx_session_updates_clone.clone();
                let active_conns_for_timeout = active_conns_clone.clone();
                let encryption_keys_for_timeout = encryption_keys_for_conn.clone();
                let peer_disconnected_for_timeout = peer_disconnected.clone();
                Some(tokio::spawn(async move {
                    tokio::time::sleep(handshake_timeout_for_conn).await;

                    let has_handshake = {
                        let sessions = session_map_for_timeout.read().unwrap();
                        sessions
                            .values()
                            .any(|info| info.conn_id == conn_id_for_timeout)
                    };

                    if !has_handshake {
                        let already_closed = peer_disconnected_for_timeout.load(Ordering::Relaxed);
                        if already_closed {
                            tracing::info!(
                                conn_id = conn_id_for_timeout,
                                "[Network] handshake timeout, connection already closed by peer (skip TERMINATE_RESP)"
                            );
                        } else {
                            tracing::warn!(
                                conn_id = conn_id_for_timeout,
                                timeout_secs = handshake_timeout_for_conn.as_secs(),
                                "[Network] handshake timeout, closing connection"
                            );

                            terminate::send_terminate_resp_on_close(
                                conn_id_for_timeout,
                                session_map_for_timeout.clone(),
                                encryption_keys_for_timeout.clone(),
                                active_conns_for_timeout.clone(),
                                terminate::status::ERROR_HANDSHAKE_TIMEOUT,
                            );
                        }

                        {
                            let mut signals = close_signals_for_timeout.lock().unwrap();
                            if let Some(tx) = signals.remove(&conn_id_for_timeout) {
                                let _ = tx.send(());
                            }
                        }

                        let _ =
                            tx_session_updates_for_timeout.send(SessionUpdate::RemoveByConnId {
                                conn_id: conn_id_for_timeout,
                            });
                    }
                }))
            } else {
                None
            };

            let (result, handshake_timeout) = tokio::select! {
                result = handle_connection(
                    conn_id,
                    socket,
                    addr_clone,
                    tx_requests_clone,
                    rx_socket_write,
                    cache_clone,
                    tx_session_updates_clone.clone(),
                    encryption_keys_for_conn.clone(),
                    session_map_for_handshake.clone(),
                    active_conns_clone.clone(),
                    peer_disconnected.clone(),
                    reply_in_flight_for_conn,
                ) => {
                    if let Some(handle) = handshake_timeout_handle {
                        handle.abort();
                    }
                    (result, false)
                }
                _ = rx_close_signal => {
                    tracing::info!(conn_id, "[Network] connection closed (handshake timeout)");
                    (Ok(()), true)
                }
            };

            {
                let mut signals = close_signals_for_conn.lock().unwrap();
                signals.remove(&conn_id);
            }

            let _ = tx_session_updates_clone.send(SessionUpdate::RemoveByConnId { conn_id });

            {
                let mut conns = active_conns_clone.lock().unwrap();
                conns.remove(&conn_id);
            }

            let _ = tx_conn_closed_for_conn.send(conn_id);

            drop(_conn_count_guard);

            let connection_failed = result.is_err() || handshake_timeout;
            if ip_block_enabled && ip_block_threshold > 0 {
                if connection_failed {
                    let svc = ip_block_service_clone.clone();
                    let ip = client_ip_for_block;
                    let th = ip_block_threshold;
                    let dur = ip_block_duration_secs;
                    tokio::spawn(async move {
                        svc.record_failure(ip, th, dur).await;
                    });
                } else {
                    ip_block_service_clone.record_success(client_ip_for_block);
                }
            }

            tracing::info!(
                conn_id,
                result_ok = !connection_failed,
                remaining = conn_count_for_cleanup.load(Ordering::Relaxed),
                "[Network] connection closed"
            );
        });
    }
}
