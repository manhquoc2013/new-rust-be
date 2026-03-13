//! Handler LOOKUP_VEHICLE (0x96): FE sends 110-byte request; backend returns LOOKUP_VEHICLE_RESP (0x97, 197 bytes).
//! Looks up the latest entry transaction from TRANSPORT_TRANSACTION_STAGE by etag (ordered by checkIn_datetime).
//! Returns error if: no record; checkIn_commit_datetime is missing; or record already has checkout (checkout_toll_id / checkout_datetime / checkout_commit_datetime).

use crate::constants::fe;
use crate::crypto::{create_encryptor_with_key, BlockEncryptMut, Pkcs7};
use crate::fe_protocol;
use crate::models::bect_messages::{LOOKUP_VEHICLE, LOOKUP_VEHICLE_RESP};
use crate::models::TCOCmessages::FE_REQUEST;
use crate::services::TransportTransactionStageService;
use crate::utils::{normalize_etag, timestamp_ms, wrap_encrypted_reply};
use aes;
use cbc;
use std::error::Error;
use std::time::Instant;
use tokio::io::AsyncWriteExt;

use crate::constants::checkin;

/// Parse LOOKUP_VEHICLE (0x96) from raw message. Expects at least 110 bytes.
fn parse_lookup_vehicle(data: &[u8]) -> Result<LOOKUP_VEHICLE, Box<dyn Error>> {
    if data.len() < fe_protocol::len::LOOKUP_VEHICLE {
        return Err(format!(
            "LOOKUP_VEHICLE message too short: {} bytes (need {})",
            data.len(),
            fe_protocol::len::LOOKUP_VEHICLE
        )
        .into());
    }
    let mut req = LOOKUP_VEHICLE::default();
    req.message_length = i32::from_le_bytes(data[0..4].try_into()?);
    req.command_id = i32::from_le_bytes(data[4..8].try_into()?);
    req.version_id = i32::from_le_bytes(data[8..12].try_into()?);
    req.request_id = i64::from_le_bytes(data[12..20].try_into()?);
    req.session_id = i64::from_le_bytes(data[20..28].try_into()?);
    req.timestamp = i64::from_le_bytes(data[28..36].try_into()?);
    req.tid = String::from_utf8_lossy(&data[36..60]).trim_end_matches('\0').to_string();
    req.etag = String::from_utf8_lossy(&data[60..84]).trim_end_matches('\0').to_string();
    req.station = i32::from_le_bytes(data[84..88].try_into()?);
    req.lane = i32::from_le_bytes(data[88..92].try_into()?);
    req.station_type = String::from_utf8_lossy(&data[92..93]).trim_end_matches('\0').to_string();
    req.lane_type = String::from_utf8_lossy(&data[93..94]).trim_end_matches('\0').to_string();
    if data.len() >= 102 {
        req.general1.copy_from_slice(&data[94..102]);
    }
    if data.len() >= 110 {
        req.general2.copy_from_slice(&data[102..110]);
    }
    Ok(req)
}

/// Serialize LOOKUP_VEHICLE_RESP (0x97) to 197 bytes and encrypt for FE reply.
async fn serialize_and_encrypt_lookup_vehicle_resp(
    resp: &LOOKUP_VEHICLE_RESP,
    encryptor: &cbc::Encryptor<aes::Aes128>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let cap = fe_protocol::response_lookup_vehicle_resp_len() as usize;
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
    buf.extend_from_slice(&resp.extra);
    buf.resize(cap, 0);
    let encrypted = encryptor.clone().encrypt_padded_vec_mut::<Pkcs7>(&buf);
    Ok(wrap_encrypted_reply(encrypted))
}

/// True if the stage has any checkout info (already checked out).
fn stage_has_checkout(
    checkout_toll_id: Option<i64>,
    checkout_datetime: Option<&String>,
    checkout_commit_datetime: Option<&String>,
) -> bool {
    if checkout_toll_id.map_or(false, |id| id != 0) {
        return true;
    }
    if let Some(s) = checkout_datetime {
        if !s.trim().is_empty() {
            return true;
        }
    }
    if let Some(s) = checkout_commit_datetime {
        if !s.trim().is_empty() {
            return true;
        }
    }
    false
}

/// Handle LOOKUP_VEHICLE (0x96): parse request, get latest TRANSPORT_TRANSACTION_STAGE by etag (checkIn_datetime), validate and return LOOKUP_VEHICLE_RESP (0x97).
pub async fn handle_lookup_vehicle(
    _fe_request: FE_REQUEST,
    data: &[u8],
    _conn_id: i32,
    encryption_key: &str,
    _db_pool: std::sync::Arc<r2d2::Pool<crate::configs::pool_factory::OdbcConnectionManager>>,
) -> Result<(Vec<u8>, i32), Box<dyn Error>> {
    let start = Instant::now();
    let req = parse_lookup_vehicle(data)?;
    let etag_norm = normalize_etag(&req.etag);

    let encryptor = create_encryptor_with_key(encryption_key);
    let mut resp = LOOKUP_VEHICLE_RESP::default();
    resp.message_length = fe_protocol::response_lookup_vehicle_resp_len();
    resp.command_id = fe::LOOKUP_VEHICLE_RESP;
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

    let transport_service = TransportTransactionStageService::default();
    let stage_opt = transport_service
        .find_latest_by_etag(&etag_norm)
        .await
        .map_err(|e| format!("[LookupVehicle] find_latest_by_etag failed: {}", e))?;

    let (status, process_time_ms) = match &stage_opt {
        None => {
            tracing::debug!(
                request_id = req.request_id,
                etag = %etag_norm,
                "[LookupVehicle] no TRANSPORT_TRANSACTION_STAGE record found"
            );
            resp.status = fe::NOT_FOUND_TOLL_TRANSACTION;
            (fe::NOT_FOUND_TOLL_TRANSACTION, start.elapsed().as_millis() as i32)
        }
        Some(stage) => {
            let has_commit = stage
                .checkin_commit_datetime
                .as_ref()
                .map_or(false, |s| !s.trim().is_empty());
            if !has_commit {
                tracing::debug!(
                    request_id = req.request_id,
                    etag = %etag_norm,
                    transport_trans_id = stage.transport_trans_id,
                    "[LookupVehicle] record has no checkIn_commit_datetime"
                );
                resp.status = fe::NOT_FOUND_LANE_TRANSACTION;
                (fe::NOT_FOUND_LANE_TRANSACTION, start.elapsed().as_millis() as i32)
            } else if stage_has_checkout(
                stage.checkout_toll_id,
                stage.checkout_datetime.as_ref(),
                stage.checkout_commit_datetime.as_ref(),
            ) {
                tracing::debug!(
                    request_id = req.request_id,
                    etag = %etag_norm,
                    transport_trans_id = stage.transport_trans_id,
                    "[LookupVehicle] record already has checkout info"
                );
                resp.status = fe::NOT_FOUND_ROUTE_TRANSACTION;
                (fe::NOT_FOUND_ROUTE_TRANSACTION, start.elapsed().as_millis() as i32)
            } else {
                resp.plate = stage
                    .plate
                    .as_deref()
                    .or(stage.checkin_plate.as_deref())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if resp.plate.is_empty() {
                    resp.plate = checkin::PLATE_EMPTY_SENTINEL.to_string();
                }
                resp.vehicle_type = stage
                    .vehicle_type
                    .as_deref()
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .unwrap_or(1);
                (0_i32, start.elapsed().as_millis() as i32)
            }
        }
    };

    resp.status = status;
    resp.process_time = process_time_ms;

    let reply = serialize_and_encrypt_lookup_vehicle_resp(&resp, &encryptor).await?;
    Ok((reply, resp.status))
}
