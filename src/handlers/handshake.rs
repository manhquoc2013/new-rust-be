//! SHAKE (0C) handler: parse FE_SHAKE (28 bytes), return FE_SHAKE_RESP (32 bytes) per spec 2.3.1.7.3 / 2.3.1.7.4.

use crate::constants::fe;
use crate::crypto::{create_encryptor_with_key, BlockEncryptMut, Pkcs7};
use crate::fe_protocol;
use crate::models::TCOCmessages::{FE_REQUEST, FE_SHAKE, FE_SHAKE_RESP};
use std::error::Error;
use tokio::io::AsyncWriteExt;

/// Handles SHAKE (0C): 28-byte request, 32-byte response including version_id.
pub async fn handle_handshake(
    rq: FE_REQUEST,
    data: Vec<u8>,
    conn_id: i32,
    _command_id: i32,
    encryption_key: &str,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let encryptor = create_encryptor_with_key(encryption_key);
    let decrypted = data.clone();

    if decrypted.len() < fe_protocol::len::HANDSHAKE {
        return Err(format!(
            "SHAKE message too short: {} bytes (minimum {})",
            decrypted.len(),
            fe_protocol::len::HANDSHAKE
        )
        .into());
    }

    let (version_id, req_id, sess_id) = match fe_protocol::parse_shake_ids(&decrypted) {
        Some(p) => p,
        None => {
            tracing::error!(
                conn_id,
                request_id = rq.request_id,
                data_len = decrypted.len(),
                "[Network] SHAKE message too short for header ids"
            );
            return Err("SHAKE message too short for header ids".into());
        }
    };
    let mut fe_shake: FE_SHAKE = FE_SHAKE::default();
    fe_shake.message_length = rq.message_length;
    fe_shake.command_id = rq.command_id;
    fe_shake.version_id = version_id;
    fe_shake.request_id = req_id;
    fe_shake.session_id = sess_id;

    let mut fe_shake_resp: FE_SHAKE_RESP = FE_SHAKE_RESP::default();
    fe_shake_resp.message_length = fe_protocol::SHAKE_RESP_LEN;
    fe_shake_resp.command_id = fe::SHAKE_RESP;
    fe_shake_resp.version_id = fe_shake.version_id;
    fe_shake_resp.request_id = fe_shake.request_id;
    fe_shake_resp.session_id = fe_shake.session_id;
    fe_shake_resp.status = 0;

    let cap = fe_shake_resp.message_length as usize;
    let mut buffer_write = Vec::with_capacity(cap);
    buffer_write
        .write_i32_le(fe_shake_resp.message_length)
        .await?;
    buffer_write.write_i32_le(fe_shake_resp.command_id).await?;
    fe_protocol::write_shake_resp_body(
        &mut buffer_write,
        fe_shake_resp.version_id,
        fe_shake_resp.request_id,
        fe_shake_resp.session_id,
        fe_shake_resp.status,
    )
    .await?;

    let encrypted_reply = encryptor
        .clone()
        .encrypt_padded_vec_mut::<Pkcs7>(&buffer_write);
    Ok(crate::utils::wrap_encrypted_reply(encrypted_reply))
}
