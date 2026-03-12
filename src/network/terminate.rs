//! TERMINATE_RESP: status code, create/send message when closing connection hoặc lỗi.

use crate::constants::fe;
use crate::crypto::{create_encryptor_with_key, BlockEncryptMut, Pkcs7};
use crate::models::TCOCmessages::FE_TERMINATE_RESP;
use crate::types::{ConnectionId, ConnectionMap, EncryptionKeyMap, SessionMap};

/// Mã trạng thái cho TERMINATE_RESP (re-export từ constants để code gọi terminate::status::* không đổi).
pub mod status {
    pub use crate::constants::terminate::*;
}

/// Tạo và mã hóa message TERMINATE_RESP.
pub fn create_terminate_resp_bytes(
    session_id: i64,
    encryption_key: &str,
    request_id: i64,
    status: i32,
) -> Option<Vec<u8>> {
    let fe_resp = FE_TERMINATE_RESP {
        message_length: 28, // 4+4+8+8+4 = 28 bytes
        command_id: fe::TERMINATE_RESP,
        request_id,
        session_id,
        status,
    };

    let encryptor = create_encryptor_with_key(encryption_key);
    let mut buffer_write = Vec::with_capacity(28);
    buffer_write.extend_from_slice(&fe_resp.message_length.to_le_bytes());
    buffer_write.extend_from_slice(&fe_resp.command_id.to_le_bytes());
    buffer_write.extend_from_slice(&fe_resp.request_id.to_le_bytes());
    buffer_write.extend_from_slice(&fe_resp.session_id.to_le_bytes());
    buffer_write.extend_from_slice(&fe_resp.status.to_le_bytes());

    let encrypted_reply = encryptor
        .clone()
        .encrypt_padded_vec_mut::<Pkcs7>(&buffer_write);

    let mut reply_bytes = Vec::with_capacity(encrypted_reply.len() + 4);
    let total_len = (encrypted_reply.len() + 4) as u32;
    reply_bytes.extend_from_slice(&total_len.to_le_bytes());
    reply_bytes.extend_from_slice(&encrypted_reply);

    Some(reply_bytes)
}

/// Gửi TERMINATE_RESP on close connection (nếu còn available). Spawn task riêng để không block caller.
pub fn send_terminate_resp_on_close(
    conn_id: ConnectionId,
    session_map: SessionMap,
    encryption_keys: EncryptionKeyMap,
    active_conns: ConnectionMap,
    status_code: i32,
) {
    tokio::spawn(async move {
        let session_id = {
            let sessions = session_map.read().unwrap_or_else(|e| e.into_inner());
            sessions
                .iter()
                .find(|(_, info)| info.conn_id == conn_id)
                .map(|(session_id, _)| *session_id)
        };

        let session_id = match session_id {
            Some(sid) => sid,
            None => {
                tracing::debug!(
                    conn_id,
                    "[Network] no session for conn_id, skipping TERMINATE_RESP"
                );
                return;
            }
        };

        let (encryption_key, tx) = {
            let keys = encryption_keys.lock().unwrap_or_else(|e| e.into_inner());
            let encryption_key = keys.get(&conn_id).cloned();
            drop(keys);

            let conns = active_conns.lock().unwrap_or_else(|e| e.into_inner());
            let tx = conns.get(&conn_id).cloned();
            drop(conns);

            match (encryption_key, tx) {
                (Some(key), Some(tx)) => (key, tx),
                (None, _) => {
                    tracing::debug!(
                        conn_id,
                        "[Network] no encryption_key for conn_id, skipping TERMINATE_RESP"
                    );
                    return;
                }
                (_, None) => {
                    tracing::debug!(
                        conn_id,
                        "[Network] connection already closed, skipping TERMINATE_RESP"
                    );
                    return;
                }
            }
        };

        let reply_bytes =
            match create_terminate_resp_bytes(session_id, &encryption_key, 0, status_code) {
                Some(rb) => rb,
                None => {
                    tracing::debug!(conn_id, "[Network] failed to create TERMINATE_RESP");
                    return;
                }
            };

        if tracing::enabled!(tracing::Level::DEBUG) {
            tracing::debug!(
                conn_id,
                session_id,
                request_id = 0_i64,
                command_id = fe::TERMINATE_RESP,
                status = status_code,
                "[FE] TERMINATE_RESP returning to client"
            );
        }

        match tx.send(reply_bytes).await {
            Ok(_) => {
                tracing::info!(
                    conn_id,
                    session_id,
                    status = status_code,
                    "[Network] TERMINATE_RESP sent"
                );
            }
            Err(_) => {
                tracing::debug!(
                    conn_id,
                    "[Network] failed to send TERMINATE_RESP (connection closed)"
                );
            }
        }
    });
}
