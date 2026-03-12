//! Parse and build CHECKOUT_COMMIT_BOO (3AZ) / CHECKOUT_COMMIT_BOO_RESP (3BZ) per spec 2.3.1.7.19 / 2.3.1.7.20.
//! Request: 188 bytes fixed. Response: 96 bytes fixed.

use crate::fe_protocol;
use crate::models::bect_messages::{CHECKOUT_COMMIT_BOO, CHECKOUT_COMMIT_BOO_RESP};
use crate::utils::normalize_etag;
use std::error::Error;

/// Parse CHECKOUT_COMMIT_BOO (3AZ) from FE buffer. Normalizes string fields (trim null and space).
pub fn parse_checkout_commit_boo(data: &[u8]) -> Result<CHECKOUT_COMMIT_BOO, Box<dyn Error>> {
    if data.len() < fe_protocol::len::CHECKOUT_COMMIT_BOO {
        return Err(format!(
            "CHECKOUT_COMMIT_BOO message too short: {} bytes (expected {})",
            data.len(),
            fe_protocol::len::CHECKOUT_COMMIT_BOO
        )
        .into());
    }
    let mut req = CHECKOUT_COMMIT_BOO::default();
    req.message_length = i32::from_le_bytes(data[0..4].try_into()?);
    req.command_id = i32::from_le_bytes(data[4..8].try_into()?);
    req.version_id = i32::from_le_bytes(data[8..12].try_into()?);
    req.request_id = i64::from_le_bytes(data[12..20].try_into()?);
    req.session_id = i64::from_le_bytes(data[20..28].try_into()?);
    req.timestamp = i64::from_le_bytes(data[28..36].try_into()?);
    req.tid = normalize_etag(&String::from_utf8_lossy(&data[36..60]));
    req.etag = normalize_etag(&String::from_utf8_lossy(&data[60..84]));
    req.ticket_in_id = i64::from_le_bytes(data[84..92].try_into()?);
    req.hub_id = i64::from_le_bytes(data[92..100].try_into()?);
    req.ticket_out_id = i64::from_le_bytes(data[100..108].try_into()?);
    req.ticket_eTag_id = i64::from_le_bytes(data[108..116].try_into()?);
    req.station_in = i32::from_le_bytes(data[116..120].try_into()?);
    req.lane_in = i32::from_le_bytes(data[120..124].try_into()?);
    req.station_out = i32::from_le_bytes(data[124..128].try_into()?);
    req.lane_out = i32::from_le_bytes(data[128..132].try_into()?);
    req.plate = normalize_etag(&String::from_utf8_lossy(&data[132..152]));
    req.trans_amount = i32::from_le_bytes(data[152..156].try_into()?);
    req.trans_datetime = i64::from_le_bytes(data[156..164].try_into()?);
    req.general1.copy_from_slice(&data[164..172]);
    req.general2.copy_from_slice(&data[172..188]);
    Ok(req)
}

/// Build CHECKOUT_COMMIT_BOO_RESP (3BZ) buffer: 96 bytes per spec 2.3.1.7.20.
pub fn build_checkout_commit_boo_resp(resp: &CHECKOUT_COMMIT_BOO_RESP) -> Vec<u8> {
    let cap = fe_protocol::response_checkout_commit_boo_resp_len() as usize;
    let mut buf = Vec::with_capacity(cap);
    buf.extend_from_slice(&(resp.message_length as i32).to_le_bytes());
    buf.extend_from_slice(&(resp.command_id as i32).to_le_bytes());
    buf.extend_from_slice(&(resp.version_id as i32).to_le_bytes());
    buf.extend_from_slice(&resp.request_id.to_le_bytes());
    buf.extend_from_slice(&resp.session_id.to_le_bytes());
    buf.extend_from_slice(&resp.timestamp.to_le_bytes());
    buf.extend_from_slice(&resp.ticket_in_id.to_le_bytes());
    buf.extend_from_slice(&resp.hub_id.to_le_bytes());
    buf.extend_from_slice(&resp.ticket_eTag_id.to_le_bytes());
    buf.extend_from_slice(&resp.ticket_out_id.to_le_bytes());
    buf.extend_from_slice(&(resp.status as i32).to_le_bytes());
    buf.extend_from_slice(&resp.general1);
    buf.extend_from_slice(&resp.general2);
    buf.resize(cap, 0);
    buf
}
