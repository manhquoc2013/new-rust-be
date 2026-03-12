//! Serialize and encrypt FE_ROLLBACK_RESP.

use crate::crypto::BlockEncryptMut;
use crate::crypto::Pkcs7;
use crate::fe_protocol;
use crate::models::TCOCmessages::FE_ROLLBACK_RESP;
use aes;
use cbc;
use std::error::Error;
use tokio::io::AsyncWriteExt;

/// Serialize and encrypt FE_ROLLBACK_RESP.
pub(crate) async fn serialize_and_encrypt_rollback_response(
    fe_resp: &FE_ROLLBACK_RESP,
    encryptor: &cbc::Encryptor<aes::Aes128>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    if tracing::enabled!(tracing::Level::DEBUG) {
        tracing::debug!(
            request_id = fe_resp.request_id,
            session_id = fe_resp.session_id,
            command_id = fe_resp.command_id,
            status = fe_resp.status,
            "[FE] ROLLBACK_RESP returning to client"
        );
    }
    let cap = fe_protocol::response_header_status_len() as usize;
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
