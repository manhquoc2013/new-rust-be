//! Handler QUERY_VEHICLE_BOO (1A): FE sends 122-byte request; backend returns QUERY_VEHICLE_BOO_RESP (1B, 133 bytes).
//! Resolves vehicle and account from subscriber by etag; normalizes content per spec 2.3.1.7.13 / 2.3.1.7.14.

use crate::constants::{bect, fe};
use crate::crypto::{create_encryptor_with_key, BlockEncryptMut, Pkcs7};
use crate::db::repositories::account_repository::get_account_for_charge;
use crate::db::repositories::subscriber_repository::get_subscriber_by_etag;
use crate::fe_protocol;
use crate::models::bect_messages::{QUERY_VEHICLE_BOO, QUERY_VEHICLE_BOO_RESP};
use crate::models::TCOCmessages::FE_REQUEST;
use crate::utils::{normalize_etag, timestamp_ms, wrap_encrypted_reply};
use aes;
use cbc;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::AsyncWriteExt;

use crate::configs::pool_factory::OdbcConnectionManager;
use crate::constants::checkin;
use r2d2::Pool;

/// Parse QUERY_VEHICLE_BOO (1A) from raw message. Expects at least 122 bytes.
fn parse_query_vehicle_boo(data: &[u8]) -> Result<QUERY_VEHICLE_BOO, Box<dyn Error>> {
    if data.len() < fe_protocol::len::QUERY_VEHICLE_BOO {
        return Err(format!(
            "QUERY_VEHICLE_BOO message too short: {} bytes (need {})",
            data.len(),
            fe_protocol::len::QUERY_VEHICLE_BOO
        )
        .into());
    }
    let mut req = QUERY_VEHICLE_BOO::default();
    req.message_length = i32::from_le_bytes(data[0..4].try_into()?);
    req.command_id = i32::from_le_bytes(data[4..8].try_into()?);
    req.version_id = i32::from_le_bytes(data[8..12].try_into()?);
    req.request_id = i64::from_le_bytes(data[12..20].try_into()?);
    req.session_id = i64::from_le_bytes(data[20..28].try_into()?);
    req.timestamp = i64::from_le_bytes(data[28..36].try_into()?);
    req.tid = String::from_utf8_lossy(&data[36..60])
        .trim_end_matches('\0')
        .to_string();
    req.etag = String::from_utf8_lossy(&data[60..84])
        .trim_end_matches('\0')
        .to_string();
    req.station = i32::from_le_bytes(data[84..88].try_into()?);
    req.lane = i32::from_le_bytes(data[88..92].try_into()?);
    req.station_type = String::from_utf8_lossy(&data[92..93])
        .trim_end_matches('\0')
        .to_string();
    req.lane_type = String::from_utf8_lossy(&data[93..94])
        .trim_end_matches('\0')
        .to_string();
    req.min_balance = i32::from_le_bytes(data[94..98].try_into()?);
    if data.len() >= 106 {
        req.general1.copy_from_slice(&data[98..106]);
    }
    if data.len() >= 122 {
        req.general2.copy_from_slice(&data[106..122]);
    }
    Ok(req)
}

/// Serialize QUERY_VEHICLE_BOO_RESP (1B) to 133 bytes and encrypt for FE reply.
async fn serialize_and_encrypt_query_vehicle_boo_resp(
    resp: &QUERY_VEHICLE_BOO_RESP,
    encryptor: &cbc::Encryptor<aes::Aes128>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let cap = fe_protocol::response_query_vehicle_boo_resp_len() as usize;
    let mut buf = Vec::with_capacity(cap);
    buf.write_i32_le(resp.message_length).await?;
    buf.write_i32_le(resp.command_id).await?;
    buf.write_i32_le(resp.version_id).await?;
    buf.write_i64_le(resp.request_id).await?;
    buf.write_i64_le(resp.session_id).await?;
    buf.write_i64_le(resp.timestamp).await?;
    buf.write_i32_le(resp.process_time).await?;
    let mut etag_buf = [0u8; 24];
    let etag_bytes = resp.etag.as_bytes();
    let len = etag_bytes.len().min(24);
    etag_buf[..len].copy_from_slice(&etag_bytes[..len]);
    buf.extend_from_slice(&etag_buf);
    buf.write_i32_le(resp.vehicle_type).await?;
    let ticket_type_byte = resp.ticket_type.chars().next().unwrap_or('L') as u8;
    buf.push(ticket_type_byte);
    let mut reg_buf = [0u8; 10];
    let reg_bytes = resp.register_vehicle_type.as_bytes();
    let rlen = reg_bytes.len().min(10);
    reg_buf[..rlen].copy_from_slice(&reg_bytes[..rlen]);
    buf.extend_from_slice(&reg_buf);
    buf.write_i32_le(resp.seat).await?;
    buf.write_i32_le(resp.weight_goods).await?;
    buf.write_i32_le(resp.weight_all).await?;
    let mut plate_buf = [0u8; 10];
    let plate_bytes = resp.plate.as_bytes();
    let plen = plate_bytes.len().min(10);
    plate_buf[..plen].copy_from_slice(&plate_bytes[..plen]);
    buf.extend_from_slice(&plate_buf);
    buf.write_i32_le(resp.status).await?;
    buf.write_i32_le(resp.min_balance_status).await?;
    buf.extend_from_slice(&resp.general1);
    buf.extend_from_slice(&resp.general2);
    buf.resize(cap, 0);
    let encrypted = encryptor.clone().encrypt_padded_vec_mut::<Pkcs7>(&buf);
    Ok(wrap_encrypted_reply(encrypted))
}

/// Handle QUERY_VEHICLE_BOO (1A): parse request, resolve subscriber/account, build and return QUERY_VEHICLE_BOO_RESP (1B).
pub async fn handle_query_vehicle_boo(
    _fe_request: FE_REQUEST,
    data: &[u8],
    _conn_id: i32,
    encryption_key: &str,
    db_pool: Arc<Pool<OdbcConnectionManager>>,
) -> Result<(Vec<u8>, i32), Box<dyn Error>> {
    let start = Instant::now();
    let req = parse_query_vehicle_boo(data)?;
    let etag_norm = normalize_etag(&req.etag);

    let encryptor = create_encryptor_with_key(encryption_key);
    let mut resp = QUERY_VEHICLE_BOO_RESP::default();
    resp.message_length = fe_protocol::response_query_vehicle_boo_resp_len();
    resp.command_id = fe::QUERY_VEHICLE_BOO_RESP;
    resp.version_id = req.version_id;
    resp.request_id = req.request_id;
    resp.session_id = req.session_id;
    resp.timestamp = timestamp_ms();
    resp.etag = req.etag.clone();
    resp.vehicle_type = 1;
    resp.ticket_type = "L".to_string();
    resp.register_vehicle_type = String::new();
    resp.seat = 0;
    resp.weight_goods = 0;
    resp.weight_all = 0;
    resp.plate = checkin::PLATE_EMPTY_SENTINEL.to_string();
    resp.min_balance_status = 0;

    let subscriber_opt = {
        let pool = db_pool.clone();
        let etag = etag_norm.clone();
        tokio::task::spawn_blocking(move || get_subscriber_by_etag(&pool, &etag))
            .await
            .unwrap_or(Ok(None))
            .unwrap_or(None)
    };

    let (status, process_time_ms) = match &subscriber_opt {
        Some(sub) => {
            resp.plate = sub.plate.trim().to_string();
            if resp.plate.is_empty() {
                resp.plate = checkin::PLATE_EMPTY_SENTINEL.to_string();
            }
            resp.vehicle_type = sub.vehicle_type.parse::<i32>().unwrap_or(1);

            if req.min_balance > 0 && sub.account_id > 0 {
                let pool = db_pool.clone();
                let account_id = sub.account_id;
                let acc_opt =
                    tokio::task::spawn_blocking(move || get_account_for_charge(&pool, account_id))
                        .await
                        .unwrap_or(Ok(None))
                        .unwrap_or(None);
                if let Some(acc) = acc_opt {
                    if acc.available_balance < req.min_balance as f64 {
                        resp.min_balance_status = 1;
                    }
                }
            }
            (0_i32, start.elapsed().as_millis() as i32)
        }
        None => {
            tracing::debug!(
                request_id = req.request_id,
                etag = %etag_norm,
                "[QueryVehicleBoo] subscriber not found"
            );
            resp.status = bect::SUBSCRIBER_NOT_FOUND;
            (
                bect::SUBSCRIBER_NOT_FOUND,
                start.elapsed().as_millis() as i32,
            )
        }
    };

    resp.status = status;
    resp.process_time = process_time_ms;

    let reply = serialize_and_encrypt_query_vehicle_boo_resp(&resp, &encryptor).await?;
    Ok((reply, resp.status))
}
