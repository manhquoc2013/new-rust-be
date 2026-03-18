//! Request processor worker: receives RequestToProcess from reader channel, runs process_request, sends ReplyToRoute to router.
//! Reader and reply writer run in separate tasks; this task processes requests sequentially (bounded by reply_in_flight). Per-connection reply order is preserved via reader's reply_received gate.

use crate::cache::config::cache_manager::CacheManager;
use crate::constants::fe;
use crate::fe_protocol;
use crate::handlers::connect::build_connect_error_response;
use crate::handlers::processor::process_request;
use crate::types::{ReplyToRoute, RequestToProcess, SessionUpdateSender};
use std::error::Error;
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::Semaphore;

/// Consumes the error and returns its message (avoids holding Box<dyn Error> across await in caller).
fn error_message(e: Box<dyn Error>) -> String {
    e.to_string()
}

/// Runs the processor loop: recv RequestToProcess, process_request, send ReplyToRoute. One request at a time; back-pressure via reply_in_flight.
pub async fn run_request_processor(
    mut rx_request_to_process: Receiver<RequestToProcess>,
    tx_logic_replies: Sender<ReplyToRoute>,
    cache: Arc<CacheManager>,
    tx_session_updates: SessionUpdateSender,
    reply_in_flight: Option<Arc<Semaphore>>,
) {
    tracing::info!("[Processor] request processor task started");

    while let Some(req) = rx_request_to_process.recv().await {
        let _permit = if let Some(ref sem) = reply_in_flight {
            match sem.clone().acquire_owned().await {
                Ok(p) => Some(p),
                Err(_) => {
                    tracing::warn!(
                        conn_id = req.conn_id,
                        "[Processor] reply_in_flight semaphore closed, dropping request"
                    );
                    continue;
                }
            }
        } else {
            None
        };

        let command_id = if req.decrypted.len() >= 8 {
            i32::from_le_bytes(req.decrypted[4..8].try_into().unwrap_or([0; 4]))
        } else {
            0
        };
        let request_id = fe_protocol::request_id_from_decrypted(&req.decrypted, command_id);
        let need_connect_err = command_id == fe::CONNECT && req.decrypted.len() >= 20;
        let (version_id, req_id_connect) = if req.decrypted.len() >= 20 {
            (
                i32::from_le_bytes(req.decrypted[8..12].try_into().unwrap_or([0; 4])),
                i64::from_le_bytes(req.decrypted[12..20].try_into().unwrap_or([0; 8])),
            )
        } else {
            (0, 0)
        };

        let (reply_to_send, err_msg_opt) = match process_request(
            req.decrypted,
            req.conn_id,
            command_id,
            cache.clone(),
            req.toll_id,
            tx_session_updates.clone(),
            &req.encryption_key_str,
            req.client_ip,
        )
        .await
        {
            Ok(reply_bytes) => (
                Some(ReplyToRoute {
                    conn_id: req.conn_id,
                    request_id,
                    data: reply_bytes,
                }),
                None,
            ),
            Err(e) => (
                need_connect_err
                    .then(|| {
                        build_connect_error_response(
                            &req.encryption_key_str,
                            version_id,
                            req_id_connect,
                            fe::ERROR_REASON_SYSTEM,
                        )
                        .ok()
                        .map(|err_resp| ReplyToRoute {
                            conn_id: req.conn_id,
                            request_id: req_id_connect,
                            data: err_resp,
                        })
                    })
                    .flatten(),
                Some(error_message(e)),
            ),
        };

        if let Some(reply) = reply_to_send {
            if tx_logic_replies.send(reply).await.is_err() {
                tracing::error!(
                    conn_id = req.conn_id,
                    request_id,
                    "[Processor] cannot send reply to router (channel closed)"
                );
            }
        }
        if let Some(err_msg) = err_msg_opt {
            tracing::error!(
                conn_id = req.conn_id,
                request_id,
                command_id = %format!("0x{:02X}", command_id),
                error = %err_msg,
                "[Processor] process_request failed"
            );
        }
    }

    tracing::info!("[Processor] request processor task ended");
}
