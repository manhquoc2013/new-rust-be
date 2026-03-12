//! Serialize and encrypt FE_ROLLBACK_RESP.
//! Cặp msg req/resp từ FE: FE gửi ROLLBACK (req, 3A/0x6A) → processor gọi handle_rollback → handler trả ROLLBACK_RESP (resp, 3B/0x6B) cho FE.

use crate::constants::fe;
use crate::crypto::BlockEncryptMut;
use crate::crypto::Pkcs7;
use crate::fe_protocol;
use crate::models::bect_messages::CHECKIN_ROLLBACK_BOO;
use crate::models::TCOCmessages::{FE_ROLLBACK, FE_ROLLBACK_RESP};
use crate::models::ETDR::ETDR;
use crate::utils::timestamp_ms;
use aes;
use cbc;
use std::error::Error;
use tokio::io::AsyncWriteExt;

fn pad_str(s: &str, len: usize) -> String {
    let mut out = s.to_string();
    out.truncate(len);
    while out.len() < len {
        out.push('\0');
    }
    out
}

/// Build CHECKIN_ROLLBACK_BOO (3A, 152 bytes) for VDTC/VETC rollback. Highway Back-End sends to Card-issuer Back-End to rollback check-in. Caller must send to BOO; on CHECKIN_ROLLBACK_BOO_RESP (3B) use status (0: success).
#[allow(dead_code)]
pub(crate) fn build_checkin_rollback_boo(
    fe_rollback: &FE_ROLLBACK,
    etdr: &ETDR,
    session_id: i64,
    version_id: i32,
) -> CHECKIN_ROLLBACK_BOO {
    let now = timestamp_ms();
    let station_type = etdr
        .boo_toll_type
        .as_deref()
        .map(|s| s.chars().next().unwrap_or('C'))
        .unwrap_or('C');
    let lane_type = etdr
        .boo_lane_type
        .as_deref()
        .map(|s| s.chars().next().unwrap_or('I'))
        .unwrap_or('I');
    CHECKIN_ROLLBACK_BOO {
        message_length: 152,
        command_id: fe::CHECKIN_ROLLBACK_BOO,
        version_id,
        request_id: fe_rollback.request_id,
        session_id,
        timestamp: now,
        ticket_id: etdr.ticket_id,
        ref_trans_id: etdr.ref_trans_id,
        tid: pad_str(etdr.t_id.trim(), 24),
        etag: pad_str(etdr.etag_number.trim(), 24),
        station: fe_rollback.station,
        station_type: pad_str(&station_type.to_string(), 1),
        lane_type: pad_str(&lane_type.to_string(), 1),
        lane: fe_rollback.lane,
        plate_from_toll: pad_str(fe_rollback.plate.trim(), 10),
        commit_datetime: now,
        general1: [0u8; 8],
        general2: [0u8; 16],
    }
}

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
