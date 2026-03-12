//! Session map: task updates session và task cleans up idle session.

use crate::db::repositories::TcocSessionRepository;
use crate::types::{SessionMap, SessionUpdate, SessionUpdateSender};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

/// Spawn task nhận SessionUpdate và cập nhật session_map (insert/update/remove).
pub fn spawn_session_update_processor(
    mut rx_session_updates: UnboundedReceiver<SessionUpdate>,
    session_map: SessionMap,
) {
    tokio::spawn(async move {
        while let Some(update) = rx_session_updates.recv().await {
            let mut sessions = session_map.write().unwrap();
            match update {
                SessionUpdate::Insert {
                    session_id,
                    conn_id,
                    last_received_time,
                } => {
                    use crate::types::SessionInfo;
                    sessions.insert(
                        session_id,
                        SessionInfo {
                            conn_id,
                            last_received_time,
                        },
                    );
                }
                SessionUpdate::UpdateHandshake {
                    session_id,
                    last_received_time,
                } => {
                    if let Some(session_info) = sessions.get_mut(&session_id) {
                        session_info.last_received_time = last_received_time;
                    }
                }
                SessionUpdate::Remove { session_id } => {
                    let sid = session_id;
                    let now = crate::utils::now_utc_db_string();
                    let repo = TcocSessionRepository::new();
                    tokio::task::spawn_blocking(move || {
                        if let Err(e) = repo.update_logout_datetime(sid, &now) {
                            tracing::error!(session_id = sid, error = %e, "[Network] TCOC_SESSIONS update_logout_datetime failed");
                        }
                    });
                    sessions.remove(&session_id);
                }
                SessionUpdate::RemoveByConnId { conn_id } => {
                    let session_ids_to_logout: Vec<i64> = sessions
                        .iter()
                        .filter(|(_, info)| info.conn_id == conn_id)
                        .map(|(id, _)| *id)
                        .collect();
                    if !session_ids_to_logout.is_empty() {
                        let now = crate::utils::now_utc_db_string();
                        let repo = TcocSessionRepository::new();
                        tokio::task::spawn_blocking(move || {
                            for sid in session_ids_to_logout {
                                if let Err(e) = repo.update_logout_datetime(sid, &now) {
                                    tracing::error!(session_id = sid, error = %e, "[Network] TCOC_SESSIONS update_logout_datetime failed");
                                }
                            }
                        });
                    }
                    sessions.retain(|_, info| info.conn_id != conn_id);
                }
            }
        }
    });
}

/// Spawn task kiểm tra session idle; nếu client đã ngắt thì gửi Remove qua tx_session_updates.
pub fn spawn_session_idle_cleanup(
    session_map: SessionMap,
    active_conns: crate::types::ConnectionMap,
    tx_session_updates: SessionUpdateSender,
    idle_timeout: Duration,
    check_interval: Duration,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(check_interval);
        loop {
            interval.tick().await;
            let now = Instant::now();
            let mut sessions_to_cleanup = Vec::new();
            {
                let sessions = session_map.read().unwrap();
                for (session_id, session_info) in sessions.iter() {
                    if now.duration_since(session_info.last_received_time) <= idle_timeout {
                        continue;
                    }
                    sessions_to_cleanup.push((*session_id, session_info.conn_id));
                }
            }
            for (session_id, conn_id) in sessions_to_cleanup {
                let still_connected = {
                    let conns = active_conns.lock().unwrap();
                    conns.contains_key(&conn_id)
                };
                if still_connected {
                    tracing::debug!(
                        session_id,
                        conn_id,
                        idle_secs = idle_timeout.as_secs(),
                        "[Network] session idle, client still connected"
                    );
                    continue;
                }
                let _ = tx_session_updates.send(SessionUpdate::Remove { session_id });
                tracing::info!(
                    session_id,
                    conn_id,
                    idle_secs = idle_timeout.as_secs(),
                    "[Network] session idle cleaned up"
                );
            }
        }
    });
}
