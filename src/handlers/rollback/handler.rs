//! Handler ROLLBACK: FE gửi ROLLBACK (req), handler trả FE_ROLLBACK_RESP (resp).
//! Cặp req/resp từ FE: FE gửi ROLLBACK (3A/0x6A) → processor gọi handle_rollback → handler xử lý, trả ROLLBACK_RESP (3B/0x6B) cho FE.

use super::common::serialize_and_encrypt_rollback_response;
use crate::constants::fe;
use crate::fe_protocol;
use crate::models::TCOCmessages::{FE_ROLLBACK, FE_ROLLBACK_RESP};
use aes;
use cbc;
use std::error::Error;

/// Xử lý ROLLBACK (process): trả FE_ROLLBACK_RESP.
pub(crate) async fn process_rollback(
    fe_rollback: &FE_ROLLBACK,
    conn_id: i32,
    encryptor: &cbc::Encryptor<aes::Aes128>,
) -> Result<(Vec<u8>, i32), Box<dyn Error>> {
    tracing::debug!(
        conn_id,
        request_id = fe_rollback.request_id,
        "[ROLLBACK] processing rollback"
    );

    let mut fe_resp: FE_ROLLBACK_RESP = FE_ROLLBACK_RESP::default();
    fe_resp.message_length = fe_protocol::response_header_status_len();
    fe_resp.command_id = fe::ROLLBACK_RESP;
    fe_resp.request_id = fe_rollback.request_id;
    fe_resp.session_id = conn_id as i64;
    fe_resp.status = 0;

    tracing::debug!(
        conn_id,
        request_id = fe_rollback.request_id,
        status = fe_resp.status,
        "[ROLLBACK] rollback response"
    );
    let reply_bytes = serialize_and_encrypt_rollback_response(&fe_resp, encryptor).await?;
    tracing::debug!(
        conn_id,
        request_id = fe_rollback.request_id,
        reply_len = reply_bytes.len(),
        "[ROLLBACK] sending FE_ROLLBACK_RESP"
    );

    Ok((reply_bytes, fe_resp.status))
}
