//! Route reply from logic handler to the correct connection by conn_id.
//! Sends asynchronously (spawn per reply) so one slow connection does not block others.
//! When a conn_id is closed, all replies for that conn_id are dropped (closed_conn_ids set).
//! Optional: limit concurrent send tasks (semaphore) and/or timeout per send; on limit or timeout, reply is dropped.

use crate::configs::config::Config;
use crate::constants::network;
use crate::types::{ConnectionId, ConnectionMap, ReplyToRoute};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::Semaphore;

/// Run task to route reply by conn_id; each reply is sent in a spawned task so the router never blocks.
/// When a connection is closed (conn_id removed), all subsequent replies for that conn_id are dropped.
pub async fn run_connection_router(
    mut rx_logic_replies: Receiver<ReplyToRoute>,
    mut rx_conn_closed: UnboundedReceiver<ConnectionId>,
    active_conns: ConnectionMap,
) {
    let cfg = Config::get();
    let max_concurrent = cfg.reply_send_max_concurrent;
    let timeout_secs = cfg.reply_send_timeout_secs;

    let semaphore = if max_concurrent > 0 {
        Some(Arc::new(Semaphore::new(max_concurrent)))
    } else {
        None
    };

    let timeout_duration = if timeout_secs > 0 {
        Some(Duration::from_secs(timeout_secs))
    } else {
        None
    };

    let closed_conn_ids: Arc<std::sync::Mutex<HashSet<ConnectionId>>> =
        Arc::new(std::sync::Mutex::new(HashSet::new()));

    loop {
        tokio::select! {
            Some(conn_id) = rx_conn_closed.recv() => {
                let mut closed = closed_conn_ids.lock().unwrap();
                closed.insert(conn_id);
                if closed.len() > network::MAX_CLOSED_CONN_IDS {
                    closed.clear();
                }
            }
            reply = rx_logic_replies.recv() => {
                let Some(reply) = reply else { break };
                let ReplyToRoute { conn_id, request_id, data } = reply;

                if closed_conn_ids.lock().unwrap().contains(&conn_id) {
                    tracing::debug!(conn_id, request_id, "[Network] drop reply for closed connection");
                    continue;
                }

                let sender_opt = {
                    let conns = active_conns.lock().unwrap();
                    conns.get(&conn_id).cloned()
                };

                let Some(sender) = sender_opt else {
                    closed_conn_ids.lock().unwrap().insert(conn_id);
                    tracing::debug!(conn_id, request_id, "[Network] connection closed, drop reply");
                    continue;
                };

                if let Some(ref sem) = semaphore {
                    let permit = match sem.clone().try_acquire_owned() {
                        Ok(p) => p,
                        Err(_) => {
                            tracing::warn!(
                                conn_id,
                                request_id,
                                max_concurrent,
                                "[Network] reply dropped, max concurrent reply sends reached"
                            );
                            continue;
                        }
                    };
                    let timeout_dur = timeout_duration;
                    tokio::spawn(async move {
                        let _permit = permit;
                        send_with_timeout(sender, data, conn_id, request_id, timeout_dur).await;
                    });
                } else {
                    let timeout_dur = timeout_duration;
                    tokio::spawn(async move {
                        send_with_timeout(sender, data, conn_id, request_id, timeout_dur).await;
                    });
                }
            }
        }
    }
}

async fn send_with_timeout(
    sender: tokio::sync::mpsc::Sender<Vec<u8>>,
    data: Vec<u8>,
    conn_id: i32,
    request_id: i64,
    timeout_duration: Option<Duration>,
) {
    let result = if let Some(dur) = timeout_duration {
        tokio::time::timeout(dur, sender.send(data)).await
    } else {
        Ok(sender.send(data).await)
    };
    match result {
        Ok(Ok(())) => {}
        Ok(Err(_)) => {
            tracing::error!(
                conn_id,
                request_id,
                "[Network] failed to queue send to conn"
            );
        }
        Err(_) => {
            tracing::warn!(
                conn_id,
                request_id,
                timeout_secs = timeout_duration.map(|d| d.as_secs()).unwrap_or(0),
                "[Network] reply send timeout, dropped"
            );
        }
    }
}
