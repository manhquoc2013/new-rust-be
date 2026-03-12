//! Shared functions for checkin: normalize_station_type, lane_type, ticket_type, MDH_ID, serialize FE_CHECKIN_IN_RESP.
//! - `resolve_ticket_type_from_query_resp`: resolve ticket_type from query response with fallback from ETDR (returns String).
//! - `resolve_ticket_type_i32_from_query_resp`: resolve ticket_type from query response with fallback from ETDR (returns i32).
//! - `try_checkin_from_sync_boo` / `try_checkin_from_sync_bect`: sync from TRANSPORT_TRANS_STAGE_SYNC to main table when CHECKIN OUT does not find ETDR.

use crate::configs::config::Config;
use crate::constants::checkin;
use crate::constants::etdr;
use crate::constants::lane_type;
use crate::crypto::BlockEncryptMut;
use crate::crypto::Pkcs7;
use crate::db::repositories::{
    TransportTransStageSync, TransportTransStageSyncDt, TransportTransStageSyncDtRepository,
    TransportTransStageSyncRepository, TransportTransStageTcd, TransportTransactionStage,
    TransportTransactionStageRepository,
};
use crate::db::Repository;
use crate::fe_protocol;
use crate::models::TCOCmessages::{FE_CHECKIN, FE_CHECKIN_IN_RESP};
use crate::models::ETDR::{
    get_etdr_cache, get_latest_checkin_by_etag, save_bect_tcd_list_with_lock,
    save_etdr, ETDR,
};
use crate::services::transport_transaction_stage_service::TransportTransactionStageService;
use crate::utils::{now_utc_db_string, timestamp_ms};
use aes;
use cbc;
use std::error::Error;
use tokio::io::AsyncWriteExt;

/// Plate value when no plate (aligned with Java TCOCMessageChannel); used for checkin response when no other source.
pub(crate) use crate::constants::checkin::PLATE_EMPTY_SENTINEL;

/// Ensure fe_resp.plate is not empty before returning response: if empty use fe_checkin_plate (from request) or PLATE_EMPTY_SENTINEL.
#[allow(dead_code)]
pub(crate) fn ensure_fe_resp_plate(fe_resp: &mut FE_CHECKIN_IN_RESP, fe_checkin_plate: &str) {
    if fe_resp.plate.trim().is_empty() {
        let p = fe_checkin_plate.trim();
        fe_resp.plate = if p.is_empty() {
            checkin::PLATE_EMPTY_SENTINEL.to_string()
        } else {
            p.to_string()
        };
    }
}

/// Fill fe_resp from ETDR for checkout (OUT) when status != 0 so FE receives full fields (station, lane, ticket_id, ticket_type, plate, vehicle_type, price=0, price_ticket_type).
/// Call after setting fe_resp.status in error/error-like paths that have an ETDR.
#[allow(dead_code)]
pub(crate) fn fill_fe_resp_from_etdr_checkout(
    fe_resp: &mut FE_CHECKIN_IN_RESP,
    etdr: &ETDR,
    fe_checkin_plate: &str,
) {
    fe_resp.station = etdr.station_id;
    fe_resp.lane = etdr.lane_id;
    fe_resp.ticket_id = etdr.ticket_id;
    fe_resp.ticket_type = ticket_type_for_checkout_resp(etdr.ticket_type);
    fe_resp.plate = etdr.plate.trim().to_string();
    if fe_resp.plate.is_empty() {
        let p = fe_checkin_plate.trim();
        fe_resp.plate = if p.is_empty() {
            checkin::PLATE_EMPTY_SENTINEL.to_string()
        } else {
            p.to_string()
        };
    }
    fe_resp.vehicle_type = etdr.vehicle_type.parse::<i32>().unwrap_or(1);
    fe_resp.price = 0;
    fe_resp.price_ticket_type = if etdr.price_ticket_type != 0 {
        etdr.price_ticket_type
    } else {
        default_price_ticket_type()
    };
}

/// Returns true if ETDR is invalid for checkout (should return NOT_FOUND_ROUTE_TRANSACTION).
/// Checks: toll_out set, already checkout, missing commit times, zero checkin time, or same-station checkout within configured min interval.
/// ETDR checkin_datetime/checkout_datetime are Unix epoch milliseconds.
#[allow(dead_code)]
pub(crate) fn is_etdr_invalid_for_checkout(etdr: &ETDR) -> bool {
    etdr.toll_out != 0
        || etdr.checkout_datetime != 0
        || etdr.checkout_commit_datetime != 0
        || etdr.checkin_datetime == 0
}

/// Returns true if ETDR is invalid for commit IN (checkin commit, entry lane).
/// Used by VDTC/VETC commit handler when lane is IN.
#[allow(dead_code)]
pub(crate) fn is_etdr_invalid_for_commit_in(etdr: &ETDR, commit_station: i32) -> bool {
    etdr.checkin_datetime == 0
        || etdr.checkout_datetime != 0
        || etdr.checkout_commit_datetime != 0
        || etdr.station_id != commit_station
        || etdr.toll_out != 0
}

/// Returns true if ETDR is invalid for commit OUT (checkout commit, exit lane).
/// Used by VDTC/VETC commit handler when lane is OUT.
#[allow(dead_code)]
pub(crate) fn is_etdr_invalid_for_commit_out(etdr: &ETDR, commit_station: i32) -> bool {
    etdr.checkin_datetime == 0
        || etdr.checkout_datetime == 0
        || etdr.checkout_commit_datetime != 0
        || etdr.toll_out != commit_station
        || etdr.station_id == 0
}

/// station_type for protocol (VETC/VDTC/BECT): take from toll.toll_type, if empty default "C", always uppercase.
/// Returns exactly one char (protocol expects single-char; Unicode uppercase may yield multiple, take first only).
pub(crate) fn normalize_station_type(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return "C".to_string();
    }
    let uppercased = s
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>());
    match uppercased.as_deref() {
        Some(u) if !u.is_empty() => u.chars().next().unwrap_or('C').to_string(),
        _ => "C".to_string(),
    }
}

/// SYNC has no TOLL_TYPE: get from cache by checkin_toll_id, if missing default "C".
fn toll_type_from_checkin_toll_id(checkin_toll_id: Option<i64>) -> String {
    checkin_toll_id
        .and_then(|id| crate::models::TollCache::get_toll(id as i32))
        .map(|toll| normalize_station_type(&toll.toll_type))
        .unwrap_or_else(|| "C".to_string())
}

/// ETDR: if boo_toll_type present and non-empty use it (normalize); else look up toll cache by checkin_toll_id then checkout_toll_id; if still missing or empty then "C".
pub(crate) fn resolve_toll_type_for_etdr(
    boo_toll_type: Option<&String>,
    checkin_toll_id: Option<i64>,
    checkout_toll_id: Option<i64>,
) -> String {
    if let Some(s) = boo_toll_type {
        let t = s.trim();
        if !t.is_empty() {
            return normalize_station_type(t);
        }
    }
    if let Some(id) = checkin_toll_id {
        if let Some(toll) = crate::models::TollCache::get_toll(id as i32) {
            return normalize_station_type(&toll.toll_type);
        }
    }
    if let Some(id) = checkout_toll_id {
        if let Some(toll) = crate::models::TollCache::get_toll(id as i32) {
            return normalize_station_type(&toll.toll_type);
        }
    }
    "C".to_string()
}

/// lane_type for protocol (VETC/VDTC): map DB ("1"/"2") to "I"/"O", always uppercase.
#[allow(dead_code)]
pub(crate) fn lane_type_protocol_io(db_lane_type: &str) -> String {
    if db_lane_type.trim() == lane_type::IN {
        "I".to_string()
    } else {
        "O".to_string()
    }
}

#[allow(dead_code)]
/// Returns MDH_ID used consistently (from request_id of FE_CHECKIN).
pub(crate) fn get_mdh_id_from_checkin(fe_checkin: &FE_CHECKIN) -> Option<i64> {
    Some(fe_checkin.request_id)
}

/// Convert ticket type from string (L, T, Q, N) to number. Start from 1; no match returns 0.
/// Used consistently for VETC and VDTC: L=1, T=2, Q=3, N=4.
pub(crate) fn ticket_type_str_to_i32(s: &str) -> i32 {
    match s.trim() {
        "L" => 1,
        "T" => 2,
        "Q" => 3,
        "N" => 4,
        _ => 0,
    }
}

/// Convert ticket type from number to string (1=L, 2=T, 3=Q, 4=N). No match returns "L".
pub(crate) fn ticket_type_i32_to_str(n: i32) -> String {
    match n {
        1 => "L".to_string(),
        2 => "T".to_string(),
        3 => "Q".to_string(),
        4 => "N".to_string(),
        _ => "L".to_string(),
    }
}

/// Default PRICE_TICKET_TYPE when no source (VETC/VDTC QUERY_VEHICLE_BOO_RESP does not return this field).
/// Used consistently for CHECKIN message and for fe_resp when no ETDR.
#[allow(dead_code)]
pub(crate) fn default_price_ticket_type() -> i32 {
    0
}

/// Ticket_type returned to FE on checkout. When etdr.ticket_type == 0 (sync/query empty) we still use "L" for rating,
/// so return 1 (L) for FE consistency; if non-zero keep as-is.
#[allow(dead_code)]
pub(crate) fn ticket_type_for_checkout_resp(etdr_ticket_type: i32) -> i32 {
    if etdr_ticket_type == 0 {
        1 // L - same as ticket_type used for checkout fee calculation
    } else {
        etdr_ticket_type
    }
}

/// Ticket_type returned to FE on checkin (lane IN) only from query, no ETDR fallback.
/// Exact ticket type only after fee calculation (checkout); at checkin DB has no charge_type yet,
/// so do not use ETDR to avoid returning value not yet in DB.
pub(crate) fn ticket_type_from_query_only_for_checkin_resp(ticket_type_from_query: &str) -> i32 {
    if ticket_type_from_query.trim().is_empty() || ticket_type_from_query == "0" {
        1 // L - default
    } else {
        let n = ticket_type_str_to_i32(ticket_type_from_query);
        if n == 0 {
            1
        } else {
            n
        }
    }
}

/// Returns PRICE_TICKET_TYPE for TCD record from RatingDetail (VETC/VDTC/BECT).
/// Convert from number (DTO) to DB string (BL, W, 1, ...) per price_ticket_type definition.
pub(crate) fn get_price_ticket_type_from_rating_detail(price_ticket_type: i32) -> Option<String> {
    Some(price_ticket_type.to_string())
}

/// Get price_ticket_type for ETDR/checkout: prefer first value > 0 in rating_details, else use fallback.
pub(crate) fn price_ticket_type_for_etdr_from_rating_details(
    mut it: impl Iterator<Item = i32>,
    fallback: i32,
) -> i32 {
    it.find(|&v| v > 0).unwrap_or(fallback)
}

/// Resolve ticket_type from query with ETDR fallback; returns String (L, T, Q, N) for message.
/// When query empty/"0" and no ETDR, return "L" so protocol always receives L/T/Q/N.
#[allow(dead_code)]
pub(crate) async fn resolve_ticket_type_from_query_resp(
    ticket_type_from_query: &str,
    etag: &str,
    source: crate::models::ETDR::EtdrCheckinSource,
) -> String {
    if ticket_type_from_query.trim().is_empty() || ticket_type_from_query == "0" {
        if let Some(etdr) =
            crate::models::ETDR::get_latest_checkin_without_checkout(etag, source).await
        {
            ticket_type_i32_to_str(etdr.ticket_type)
        } else {
            "L".to_string()
        }
    } else {
        let n = ticket_type_str_to_i32(ticket_type_from_query);
        if n == 0 {
            "L".to_string()
        } else {
            ticket_type_i32_to_str(n)
        }
    }
}

/// Resolve ticket_type from query with ETDR fallback; returns i32 (1=L, 2=T, 3=Q, 4=N) for response.
/// Used when source already in DB (e.g. checkout); not used for fe_resp checkin (lane IN).
#[allow(dead_code)]
pub(crate) async fn resolve_ticket_type_i32_from_query_resp(
    ticket_type_from_query: &str,
    etag: &str,
    source: crate::models::ETDR::EtdrCheckinSource,
) -> i32 {
    if ticket_type_from_query.trim().is_empty() || ticket_type_from_query == "0" {
        if let Some(etdr) =
            crate::models::ETDR::get_latest_checkin_without_checkout(etag, source).await
        {
            etdr.ticket_type
        } else {
            ticket_type_str_to_i32(ticket_type_from_query)
        }
    } else {
        ticket_type_str_to_i32(ticket_type_from_query)
    }
}

/// Serialize and encrypt FE_CHECKIN_IN_RESP response.
pub(crate) async fn serialize_and_encrypt_response(
    fe_resp: &FE_CHECKIN_IN_RESP,
    encryptor: &cbc::Encryptor<aes::Aes128>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    if tracing::enabled!(tracing::Level::DEBUG) {
        tracing::debug!(
            request_id = fe_resp.request_id,
            session_id = fe_resp.session_id,
            command_id = fe_resp.command_id,
            status = fe_resp.status,
            ticket_id = fe_resp.ticket_id,
            etag = %fe_resp.etag.trim(),
            station = fe_resp.station,
            lane = fe_resp.lane,
            price = fe_resp.price,
            vehicle_type = fe_resp.vehicle_type,
            plate = %fe_resp.plate.trim(),
            ticket_type = fe_resp.ticket_type,
            "[FE] CHECKIN_RESP returning to client"
        );
    }
    let cap = fe_protocol::response_checkin_in_resp_len() as usize;
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

    let mut buffer_etag = Vec::with_capacity(24);
    buffer_etag.extend_from_slice(fe_resp.etag.as_bytes());
    buffer_etag.resize(24, 0);
    buffer_write.extend_from_slice(&buffer_etag);

    buffer_write.write_i32_le(fe_resp.station).await?;
    buffer_write.write_i32_le(fe_resp.lane).await?;
    fe_protocol::write_fe_ticket_id(&mut buffer_write, fe_resp.ticket_id).await?;
    buffer_write.write_i32_le(fe_resp.ticket_type).await?;
    buffer_write.write_i32_le(fe_resp.price).await?;
    buffer_write.write_i32_le(fe_resp.vehicle_type).await?;

    let mut buffer_plate = Vec::with_capacity(10);
    buffer_plate.extend_from_slice(fe_resp.plate.as_bytes());
    buffer_plate.resize(10, 0);
    buffer_write.extend_from_slice(&buffer_plate);

    buffer_write.write_i32_le(fe_resp.plate_type).await?;
    buffer_write.write_i32_le(fe_resp.price_ticket_type).await?;

    let encrypted_reply = encryptor
        .clone()
        .encrypt_padded_vec_mut::<Pkcs7>(&buffer_write);
    Ok(crate::utils::wrap_encrypted_reply(encrypted_reply))
}

// --- Sync from TRANSPORT_TRANS_STAGE_SYNC (used when CHECKIN OUT does not find ETDR) ---

/// Parse datetime string "YYYY-MM-DD HH24:MI:SS" to milliseconds since epoch (0 if None or parse error).
fn parse_datetime_to_ms(s: Option<&String>) -> i64 {
    let s = match s {
        Some(x) if !x.trim().is_empty() => x.as_str(),
        _ => return 0,
    };
    chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|dt| dt.and_utc().timestamp_millis())
        .unwrap_or(0)
}

/// Pending record (not checkout) chosen from sync or main table; compare time to take the closer one.
enum BestPendingRecord {
    Sync(TransportTransStageSync, Vec<TransportTransStageSyncDt>),
    Bect(TransportTransactionStage),
}

fn build_transport_stage_from_sync(
    sync: &TransportTransStageSync,
    now: &str,
    ticket_id: i64,
) -> TransportTransactionStage {
    TransportTransactionStage {
        transport_trans_id: ticket_id,
        subscriber_id: None,
        etag_id: None,
        vehicle_id: None,
        checkin_toll_id: sync.checkin_toll_id,
        checkin_lane_id: sync.checkin_lane_id,
        checkin_commit_datetime: sync.checkin_commit_datetime.clone(),
        checkin_channel: None,
        checkin_pass: Some("P".to_string()),
        checkin_pass_reason_id: None,
        checkout_toll_id: sync.checkout_toll_id,
        checkout_lane_id: sync.checkout_lane_id,
        checkout_commit_datetime: sync.checkout_commit_datetime.clone(),
        checkout_channel: None,
        checkout_pass: None,
        checkout_pass_reason_id: None,
        charge_status: None,
        charge_type: None,
        total_amount: sync.total_amount,
        account_id: None,
        account_trans_id: None,
        checkin_datetime: sync.checkin_datetime.clone(),
        checkout_datetime: sync.checkout_datetime.clone(),
        checkin_status: Some("2".to_string()),
        etag_number: sync.etag_number.clone(),
        request_id: sync.request_id,
        checkin_plate: sync.plate.clone(),
        checkin_plate_status: None,
        checkout_plate: None,
        checkout_plate_status: None,
        plate_from_toll: None,
        img_count: None,
        checkin_img_count: None,
        checkout_img_count: None,
        status: None,
        last_update: Some(now.to_string()),
        notif_scan: None,
        toll_type: Some(toll_type_from_checkin_toll_id(sync.checkin_toll_id)),
        rating_type: Some("E".to_string()),
        insert_datetime: Some(now.to_string()),
        insuff_amount: None,
        checkout_status: None,
        bitwise_scan: None,
        plate: sync.plate.clone(),
        vehicle_type: sync.vehicle_type.clone(),
        fe_vehicle_length: None,
        fe_commit_amount: None,
        fe_weight: None,
        fe_reason_id: None,
        vehicle_type_profile: None,
        boo: Some(Config::get_vehicle_boo_type(sync.etag_number.as_deref().unwrap_or("")) as i64),
        boo_transport_trans_id: None,
        subscription_ids: None,
        checkin_shift: None,
        checkout_shift: None,
        turning_code: None,
        checkin_tid: sync.tid.clone(),
        checkout_tid: None,
        charge_in: None,
        charge_trans_id: None,
        balance: None,
        charge_in_status: None,
        charge_datetime: None,
        charge_in_104: None,
        fe_online_status: None,
        mdh_id: None,
        chd_type: None,
        chd_ref_id: None,
        chd_reason: None,
        fe_trans_id: None,
        transition_close: Some("F".to_string()), // NOT NULL in DB; F = not closed
        voucher_code: None,
        voucher_used_amount: None,
        voucher_amount: None,
        transport_sync_id: Some(sync.transport_sync_id),
        // CheckIn/Checkout from SYNC: ticket_in_id from sync; ticket_out_id from sync when sync has checkout.
        ticket_in_id: sync.ticket_in_id.or(Some(ticket_id)),
        hub_id: sync.hub_id,
        ticket_eTag_id: sync.ticket_etag_id,
        ticket_out_id: sync.ticket_out_id,
        token_id: None,
        acs_account_no: None,
        boo_etag: sync.boo_etag.clone(),
        sub_charge_in: None,
        sync_status: None,
    }
}

/// Returns ticket_id (transport_trans_id) if record with TICKET_IN_ID = ticket_in_id exists (TRANSPORT_TRANSACTION_STAGE table). Idempotent for BECT checkin.
pub(crate) async fn get_existing_pending_ticket_id_for_ticket_in_id_bect(
    ticket_in_id: i64,
) -> Option<i64> {
    if ticket_in_id == 0 {
        return None;
    }
    let repo = TransportTransactionStageRepository::new();
    match tokio::task::spawn_blocking(move || repo.find_by_ticket_in_id(ticket_in_id)).await {
        Ok(Ok(Some(record))) => Some(record.transport_trans_id),
        _ => None,
    }
}

/// Same as ensure_no_duplicate_ticket_in_id_boo but for BECT (TRANSPORT_TRANSACTION_STAGE).
pub(crate) async fn ensure_no_duplicate_ticket_in_id_bect(
    ticket_in_id: i64,
    current_transport_trans_id: i64,
    request_id: i64,
    log_prefix: &str,
) -> Result<(), i32> {
    use crate::constants::fe;
    if ticket_in_id == 0 {
        return Ok(());
    }
    let service = TransportTransactionStageService::default();
    let start = std::time::Instant::now();
    match service.get_by_ticket_in_id(ticket_in_id).await {
        Ok(Some(record)) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            tracing::debug!(
                ticket_in_id,
                elapsed_ms,
                "[DB] Get best pending from sync or main boo: get BECT record by ticket_in_id"
            );
            if record.transport_trans_id != current_transport_trans_id {
                tracing::error!(
                    request_id,
                    ticket_in_id,
                    current_transport_trans_id,
                    existing_transport_trans_id = record.transport_trans_id,
                    "{} duplicate transaction: record with same TICKET_IN_ID has different TRANSPORT_TRANS_ID",
                    log_prefix
                );
                Err(fe::DUPLICATE_TRANSACTION)
            } else {
                Ok(())
            }
        }
        _ => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            tracing::debug!(
                ticket_in_id,
                elapsed_ms,
                "[DB] Get best pending from sync or main boo: get BECT record by ticket_in_id"
            );
            Ok(())
        }
    }
}

/// Returns ticket_id if etag already has checkin pending (not checkout). Ensures same etag at one time has only one ticket_id (idempotent checkin).
#[allow(dead_code)]
pub(crate) async fn get_existing_pending_ticket_id_for_etag(etag: &str) -> Option<i64> {
    let start = std::time::Instant::now();
    let etag_norm = crate::utils::normalize_etag(etag);
    let cache_opt = get_latest_checkin_by_etag(&etag_norm).await;
    let db_opt = get_best_pending_from_sync_or_main_bect(&etag_norm).await;
    let merged = merge_latest_checkin_etdr(cache_opt, db_opt);
    let elapsed_ms = start.elapsed().as_millis() as u64;
    tracing::debug!(etag = %etag, elapsed_ms, "[DB] Get existing pending ticket id for etag");
    merged
        .filter(|e| e.toll_out == 0 && e.checkout_datetime == 0 && e.ticket_id != 0)
        .map(|e| e.ticket_id)
}

/// On checkout: merge ETDR from cache and from DB/sync, take record with latest checkIn; if same second prefer record from DB (BOO/BECT).
pub(crate) fn merge_latest_checkin_etdr(
    cache_opt: Option<ETDR>,
    db_opt: Option<ETDR>,
) -> Option<ETDR> {
    let start = std::time::Instant::now();
    let result = match (cache_opt, db_opt) {
        (None, None) => None,
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (Some(a), Some(b)) => {
            if b.checkin_datetime > a.checkin_datetime {
                Some(b)
            } else if a.checkin_datetime > b.checkin_datetime {
                Some(a)
            } else {
                // Same second: prefer DB (BOO/BECT)
                Some(b)
            }
        }
    };
    let elapsed_ms = start.elapsed().as_millis() as u64;
    tracing::debug!(elapsed_ms, "[DB] merge_latest_checkin_etdr");
    result
}

/// Fill info from Sync message into existing ETDR (etag + ticket_in_id match): plate, vehicle_type, ticket_type, checkin/checkout datetime and commit datetimes, station_id, lane_id, toll_out, lane_out, request_id, sync_status, ...
/// Shared for BECT and BOO (same source TransportTransStageSync).
fn fill_etdr_from_sync(
    etdr: &mut ETDR,
    sync: &TransportTransStageSync,
    _details: &[TransportTransStageSyncDt],
) {
    if let Some(ref p) = sync.plate {
        if !p.trim().is_empty() {
            etdr.plate = p.trim().to_string();
        }
    }
    if let Some(ref v) = sync.vehicle_type {
        if !v.trim().is_empty() {
            etdr.vehicle_type = v.trim().to_string();
        }
    }
    if let Some(ref t) = sync.ticket_type {
        if !t.trim().is_empty() {
            etdr.ticket_type = ticket_type_str_to_i32(t);
        }
    }
    if let Some(ref pt) = sync.price_ticket_type.as_deref() {
        etdr.price_ticket_type = crate::price_ticket_type::from_db_string(Some(pt));
    }
    if let Some(ref s) = sync.checkin_datetime.as_deref() {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S") {
            etdr.checkin_datetime = dt.and_utc().timestamp_millis();
        }
    }
    if let Some(ref s) = sync.checkin_commit_datetime.as_deref() {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S") {
            etdr.checkin_commit_datetime = dt.and_utc().timestamp_millis();
        }
    }
    if let Some(id) = sync.checkin_toll_id {
        etdr.station_id = id as i32;
    }
    if let Some(id) = sync.checkin_lane_id {
        etdr.lane_id = id as i32;
    }
    if let Some(rq) = sync.request_id {
        etdr.request_id = rq;
    }
    if let Some(tid) = sync.ticket_in_id {
        etdr.ticket_in_id = Some(tid);
    }
    if let Some(eid) = sync.ticket_etag_id {
        etdr.ticket_eTag_id = Some(eid);
    }
    if let Some(hid) = sync.hub_id {
        etdr.hub_id = Some(hid);
    }
    // Checkout from sync (when SYNC has checkout info).
    if let Some(id) = sync.checkout_toll_id {
        etdr.toll_out = id as i32;
    }
    if let Some(id) = sync.checkout_lane_id {
        etdr.lane_out = id as i32;
    }
    if let Some(ref s) = sync.checkout_datetime.as_deref() {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S") {
            let ms = dt.and_utc().timestamp_millis();
            etdr.checkout_datetime = ms;
            etdr.time_route_checkout = ms;
        }
    }
    if let Some(ref s) = sync.checkout_commit_datetime.as_deref() {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S") {
            etdr.checkout_commit_datetime = dt.and_utc().timestamp_millis();
        }
    }
    etdr.checkin_from_sync = true;
    if let Some(ss) = sync.sync_status {
        etdr.sync_status = ss as i32;
    }
    etdr.time_update = timestamp_ms();
}

/// Insert from pre-loaded sync to TRANSPORT_TRANSACTION_STAGE and return ETDR (BECT).
/// Idempotent by ticket_in_id: if pending record with same TICKET_IN_ID exists then reuse that ticket_id (repo insert will skip).
async fn try_checkin_from_sync_bect_with_preloaded(
    etag_for_log: &str,
    sync: TransportTransStageSync,
    details: Vec<TransportTransStageSyncDt>,
) -> Option<ETDR> {
    let request_id = sync.request_id.unwrap_or(0);
    let mut ticket_id_reused = false;
    let mut ticket_id = if let Some(tid) = sync.ticket_in_id {
        if tid != 0 {
            if let Some(id) = get_existing_pending_ticket_id_for_ticket_in_id_bect(tid).await {
                ticket_id_reused = true;
                id
            } else {
                0
            }
        } else {
            0
        }
    } else {
        0
    };
    if ticket_id == 0 {
        ticket_id = TransportTransactionStageService::default()
            .get_ticket_id_for_bect_checkin(request_id)
            .await;
    }
    if ticket_id == 0 {
        ticket_id = crate::utils::ticket_id::generate_ticket_id_async().await;
        tracing::warn!(etag = %etag_for_log, "[BECT] sync ticket_id=0, fallback generate_ticket_id");
    }
    let result = tokio::task::spawn_blocking(move || {
        let now = now_utc_db_string();
        let stage = build_transport_stage_from_sync(&sync, &now, ticket_id);
        let stage_repo = TransportTransactionStageRepository::new();
        let new_id = stage_repo.insert(&stage).ok()?;
        // Skip TCD when: (1) reuse ticket_id and repo returns same id, or (2) have ticket_in_id but repo returns different id (record already from other node).
        let insert_was_skipped = (sync.ticket_in_id.is_some_and(|tid| tid != 0)
            && new_id != ticket_id)
            || (ticket_id_reused && new_id == ticket_id);
        let tcd_list: Vec<TransportTransStageTcd> = if insert_was_skipped {
            Vec::new()
        } else {
            details
                .iter()
                .map(|dt| {
                    let price_ticket_type = dt
                        .price_ticket_type
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .or_else(|| Some(crate::price_ticket_type::to_db_string(1)));
                    TransportTransStageTcd {
                        transport_stage_tcd_id: 0,
                        transport_trans_id: new_id,
                        boo: dt.boo.unwrap_or(1),
                        bot_id: dt.bot_id,
                        toll_a_id: dt.toll_a_id,
                        toll_b_id: dt.toll_b_id,
                        stage_id: dt.stage_id,
                        price_id: dt.price_id,
                        ticket_type: dt.ticket_type.clone(),
                        price_ticket_type,
                        price_amount: dt.price_amount,
                        subscription_id: dt.subscription_id.clone(),
                        vehicle_type: dt.vehicle_type,
                        mdh_id: None,
                        checkin_datetime: dt.checkin_datetime.clone(),
                    }
                })
                .collect()
        };
        stage_repo
            .find_by_id(new_id)
            .ok()
            .flatten()
            .map(|r| (r, insert_was_skipped, tcd_list))
    })
    .await;
    match result {
        Ok(Some((record, insert_was_skipped, tcd_list))) => {
            if !insert_was_skipped && !tcd_list.is_empty() {
                let new_id = record.transport_trans_id;
                if let Err(e) = save_bect_tcd_list_with_lock(new_id, &tcd_list).await {
                    tracing::warn!(transport_trans_id = new_id, error = %e, "[BECT] TCD save_tcd_list (from sync) failed");
                }
            }
            let etdr = ETDR::from_transport_transaction_stage(&record);
            save_etdr(etdr.clone());
            tracing::info!(etag = %etag_for_log, transport_trans_id = record.transport_trans_id, "[BECT] checkin from TRANSPORT_TRANS_STAGE_SYNC, inserted TRANSPORT_TRANSACTION_STAGE and ETDR cache");
            Some(etdr)
        }
        Ok(None) => {
            tracing::debug!(etag = %etag_for_log, "[BECT] insert failed for checkin from sync");
            None
        }
        Err(e) => {
            tracing::warn!(etag = %etag_for_log, error = %e, "[BECT] try_checkin_from_sync_bect insert task failed");
            None
        }
    }
}

/// Get checkIn record latest by etag from sync + TRANSPORT_TRANSACTION_STAGE (including already checkout), so checkout can return error if already checkout.
/// Prefer BECT when (checkIn BECT + 5s) > checkIn Sync; same transaction (transport_sync_id match) always prefer BECT.
pub(crate) async fn get_best_pending_from_sync_or_main_bect(etag: &str) -> Option<ETDR> {
    let etag_trimmed = crate::utils::normalize_etag(etag);
    let etag_for_log = etag_trimmed.clone();
    let best = tokio::task::spawn_blocking(move || {
        let sync_repo = TransportTransStageSyncRepository::new();
        let sync_opt = sync_repo.find_latest_by_etag(&etag_trimmed).ok().flatten();
        let sync_with_details = sync_opt.map(|sync| {
            let sync_dt_repo = TransportTransStageSyncDtRepository::new();
            let details = sync_dt_repo
                .find_by_transport_sync_id(sync.transport_sync_id)
                .ok()
                .unwrap_or_default();
            (sync, details)
        });
        let tts_repo = TransportTransactionStageRepository::new();
        let tts_opt = tts_repo.find_latest_by_etag(&etag_trimmed).ok().flatten();
        let ts_sync = sync_with_details
            .as_ref()
            .map(|(s, _)| {
                parse_datetime_to_ms(s.checkin_datetime.as_ref())
                    .max(parse_datetime_to_ms(s.insert_datetime.as_ref()))
            })
            .unwrap_or(0);
        let ts_tts = tts_opt
            .as_ref()
            .map(|t| {
                parse_datetime_to_ms(t.checkin_datetime.as_ref())
                    .max(parse_datetime_to_ms(t.insert_datetime.as_ref()))
            })
            .unwrap_or(0);
        // If TTS record has transport_sync_id = sync's id (same transaction, already in main table), prefer TTS.
        if let (Some((ref sync, _)), Some(tts)) = (sync_with_details.as_ref(), tts_opt.as_ref()) {
            if tts.transport_sync_id == Some(sync.transport_sync_id) {
                return Some(BestPendingRecord::Bect(tts.clone()));
            }
        }
        // If checkIn(BECT) + 5s > checkIn(Sync) then take BECT (avoid insert from sync when BECT record for same transaction exists).
        if let Some(tts) = tts_opt {
            if ts_tts + etdr::BEST_RECORD_BOO_SYNC_WINDOW_MS > ts_sync {
                Some(BestPendingRecord::Bect(tts))
            } else {
                sync_with_details.map(|(sync, details)| BestPendingRecord::Sync(sync, details))
            }
        } else {
            sync_with_details.map(|(sync, details)| BestPendingRecord::Sync(sync, details))
        }
    })
    .await
    .ok()
    .flatten()?;
    match best {
        BestPendingRecord::Sync(sync, details) => {
            // If Sync message exists and etag + ticket_in_id already in ETDR then fill info from Sync into ETDR and return.
            if let Some(ticket_in_id) = sync.ticket_in_id {
                let all = get_etdr_cache().get_all_by_etag(&etag_for_log);
                if let Some(existing) = all
                    .into_iter()
                    .find(|e| e.ticket_in_id == Some(ticket_in_id))
                {
                    let ref_trans_id = existing.ref_trans_id;
                    get_etdr_cache().update_etdr(&etag_for_log, ref_trans_id, |e| {
                        fill_etdr_from_sync(e, &sync, &details);
                    });
                    let updated = get_etdr_cache()
                        .get_all_by_etag(&etag_for_log)
                        .into_iter()
                        .find(|e| e.ref_trans_id == ref_trans_id);
                    if let Some(etdr) = updated {
                        save_etdr(etdr.clone()); // sync to KeyDB
                        tracing::info!(etag = %etag_for_log, ticket_in_id, "[BECT] ETDR already existed (etag + ticket_in_id), filled from Sync");
                        return Some(etdr);
                    }
                }
            }
            try_checkin_from_sync_bect_with_preloaded(&etag_for_log, sync, details).await
        }
        BestPendingRecord::Bect(record) => {
            let etdr = ETDR::from_transport_transaction_stage(&record);
            save_etdr(etdr.clone());
            tracing::debug!(etag = %etag_for_log, transport_trans_id = record.transport_trans_id, "[BECT] ETDR from TRANSPORT_TRANSACTION_STAGE (newer than sync), saved to cache");
            Some(etdr)
        }
    }
}
