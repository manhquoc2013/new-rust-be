//! TERMINATE handler: FE gửi TERMINATE (req), backend trả TERMINATE_RESP (resp). Serialize/encrypt FE_TERMINATE_RESP, remove session.

use crate::constants::fe;
use crate::crypto::{create_encryptor_with_key, BlockEncryptMut, Pkcs7};
use crate::fe_protocol;
use crate::models::TCOCmessages::{FE_REQUEST, FE_TERMINATE, FE_TERMINATE_RESP};
use crate::types::{SessionUpdate, SessionUpdateSender};
use aes;
use cbc;
use std::error::Error;
use tokio::io::AsyncWriteExt;

/// Serialize and encrypt FE_TERMINATE_RESP.
async fn serialize_and_encrypt_terminate_response(
    fe_resp: &FE_TERMINATE_RESP,
    encryptor: &cbc::Encryptor<aes::Aes128>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let cap = fe_resp.message_length as usize;
    let mut buffer_write = Vec::with_capacity(cap);
    buffer_write.write_i32_le(fe_resp.message_length).await?;
    buffer_write.write_i32_le(fe_resp.command_id).await?;
    fe_protocol::write_fe_request_id_session_id(
        &mut buffer_write,
        fe_resp.request_id,
        fe_resp.session_id,
    )
    .await?;
    buffer_write.write_i32_le(fe_resp.status).await?;

    let encrypted_reply = encryptor
        .clone()
        .encrypt_padded_vec_mut::<Pkcs7>(&buffer_write);
    Ok(crate::utils::wrap_encrypted_reply(encrypted_reply))
}

/// Xử lý TERMINATE command (24-byte format).
pub async fn handle_terminate(
    rq: FE_REQUEST,
    data: Vec<u8>,
    conn_id: i32,
    tx_session_updates: SessionUpdateSender,
    encryption_key: &str,
) -> Result<(Vec<u8>, i32), Box<dyn Error>> {
    let encryptor = create_encryptor_with_key(encryption_key);
    let decrypted = data.clone();
    if decrypted.len() < fe_protocol::len::TERMINATE {
        return Err(format!(
            "TERMINATE message too short: {} bytes (minimum {})",
            decrypted.len(),
            fe_protocol::len::TERMINATE
        )
        .into());
    }

    let (req_id, sess_id) = match fe_protocol::parse_request_id_session_id(&decrypted) {
        Some(p) => p,
        None => return Err("TERMINATE message too short for header ids".into()),
    };
    let mut fe_terminate: FE_TERMINATE = FE_TERMINATE::default();
    fe_terminate.message_length = rq.message_length;
    fe_terminate.command_id = rq.command_id;
    fe_terminate.request_id = req_id;
    fe_terminate.session_id = sess_id;

    tracing::debug!(
        conn_id,
        request_id = fe_terminate.request_id,
        "[Network] FE_TERMINATE decrypted"
    );

    let mut fe_resp: FE_TERMINATE_RESP = FE_TERMINATE_RESP::default();
    fe_resp.message_length = fe_protocol::response_header_status_len();
    fe_resp.command_id = fe::TERMINATE_RESP;
    fe_resp.request_id = fe_terminate.request_id;
    fe_resp.session_id = conn_id as i64;
    fe_resp.status = 0;

    tracing::debug!(
        conn_id,
        request_id = fe_terminate.request_id,
        status = fe_resp.status,
        "[Network] TERMINATE response"
    );

    {
        let session_id = fe_terminate.session_id;
        let _ = tx_session_updates.send(SessionUpdate::Remove { session_id });
        tracing::debug!(
            conn_id,
            request_id = fe_terminate.request_id,
            session_id = fe_terminate.session_id,
            "[Network] session removed"
        );
    }

    let reply_bytes = serialize_and_encrypt_terminate_response(&fe_resp, &encryptor).await?;

    tracing::debug!(
        conn_id,
        request_id = fe_terminate.request_id,
        reply_len = reply_bytes.len(),
        "[Network] sending FE_TERMINATE_RESP"
    );

    Ok((reply_bytes, fe_resp.status))
}
