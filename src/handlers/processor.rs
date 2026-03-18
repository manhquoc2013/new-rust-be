//! Route FE requests by command_id: connect, handshake, checkin, commit, rollback, terminate.
//!
//! **Luồng bản tin:** Tất cả đều cặp msg req/resp từ FE: FE gửi req (CONNECT, HANDSHAKE, CHECKIN, COMMIT, ROLLBACK, TERMINATE), processor phân command_id và gọi handler tương ứng, handler trả resp cho FE.
//!
//! **Throughput flow:** TCOC_REQUEST is saved (await) before dispatch; after handler returns, TCOC_REQUEST toll_in/toll_out and TCOC_RESPONSE/ROAMING run in background (spawn, non-blocking) to return results to FE quickly.

use crate::cache::config::cache_manager::CacheManager;
use crate::configs::rating_db::RATING_DB;
use crate::constants::{fe, fe_response_command_id, FE_VALID_COMMANDS};
use crate::fe_protocol;
use crate::models::TCOCmessages::FE_REQUEST;
use crate::services::{TcocRequestService, TcocResponseService};
use crate::types::{SessionUpdate, SessionUpdateSender};
use crate::utils::is_toll_id_valid;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

use super::checkin::handle_checkin;
use super::checkout_commit::handle_checkout_commit_boo;
use super::checkout_reserve::handle_checkout_reserve_boo;
use super::checkout_rollback::handle_checkout_rollback_boo;
use super::commit::handle_commit;
use super::connect::handle_connect;
use super::handshake::handle_handshake;
use super::lookup_vehicle::handle_lookup_vehicle;
use super::query_vehicle_boo::handle_query_vehicle_boo;
use super::rollback::handle_rollback;
use super::terminate::handle_terminate;

/// Parsed request data used for dispatch and writing ROAMING_RESPONSE.
#[derive(Clone)]
struct RequestContext {
    conn_id: i32,
    request_id: i64,
    session_id: i64,
    toll_id: Option<i64>,
    etag_id: Option<String>,
    lane_id: Option<i64>,
    plate: Option<String>,
    request_received_ms: i64,
    response_ticket_id: Option<i64>,
    /// ID of TCOC_REQUEST row for this invocation; when request_id is reused (e.g. retry), used to update the correct row.
    tcoc_request_id: Option<i64>,
}

/// Save TCOC_RESPONSE when sending response to FE.
/// response_send_ms: time when response is ready (captured by caller so PROCESS_DURATION = total client request→response; must be >= ROAMING BOO call time).
#[allow(clippy::too_many_arguments)]
fn save_tcoc_response_if_applicable(
    ctx: &RequestContext,
    response_command_id: i32,
    response: &[u8],
    status: Option<i32>,
    ticket_id: Option<i64>,
    direction: Option<String>,
    station_in_for_out: Option<i64>,
    response_send_ms: i64,
) {
    let (toll_in, toll_out) = match direction.as_deref() {
        Some("I") => (ctx.toll_id, None),
        Some("O") => (station_in_for_out, ctx.toll_id),
        _ => (None, None),
    };

    // Thống nhất với TCOC: 0 = thành công; lưu "0" khi success (khi status None vẫn coi là success cho CONNECT/SHAKE).
    let status_str = status
        .map(|s| s.to_string())
        .or_else(|| Some("0".to_string()));

    let service = TcocResponseService::new();
    // Spawn async task so as not to block main flow
    let ctx_clone = RequestContext {
        conn_id: ctx.conn_id,
        request_id: ctx.request_id,
        session_id: ctx.session_id,
        toll_id: ctx.toll_id,
        etag_id: ctx.etag_id.clone(),
        lane_id: ctx.lane_id,
        plate: ctx.plate.clone(),
        request_received_ms: ctx.request_received_ms,
        response_ticket_id: ctx.response_ticket_id,
        tcoc_request_id: ctx.tcoc_request_id,
    };
    let response_clone = if response.is_empty() {
        Vec::new()
    } else {
        response.to_vec()
    };
    tokio::spawn(async move {
        if let Err(e) = service
            .save_response(
                ctx_clone.request_id,
                response_command_id,
                ctx_clone.session_id,
                &response_clone,
                status_str,
                ticket_id,
                ctx_clone.toll_id,
                toll_in,
                toll_out,
                ctx_clone.etag_id,
                ctx_clone.lane_id,
                ctx_clone.plate,
                None, // node_id
                ctx_clone.request_received_ms,
                response_send_ms,
            )
            .await
        {
            tracing::warn!(
                conn_id = ctx_clone.conn_id,
                request_id = ctx_clone.request_id,
                response_command_id = %format!("0x{:02X}", response_command_id),
                error = %e,
                "[Processor] TCOC_RESPONSE save failed"
            );
        }
    });
}

/// After checkin/commit/rollback handler completes: update toll_in/toll_out and save TCOC_RESPONSE in background (spawn),
/// without waiting for DB so results are returned to client quickly for better throughput.
/// response_send_ms: time when response is ready (so TCOC PROCESS_DURATION = total time >= ROAMING BOO time).
#[allow(clippy::too_many_arguments)]
fn apply_post_handler_tcoc_update(
    ctx: &RequestContext,
    response_command_id: i32,
    response: &[u8],
    status: i32,
    direction: Option<String>,
    station_in_for_out: Option<i64>,
    response_ticket_id: Option<i64>,
    response_send_ms: i64,
) {
    let request_id = ctx.request_id;
    let toll_id = ctx.toll_id;
    let ctx_clone = RequestContext {
        conn_id: ctx.conn_id,
        request_id: ctx.request_id,
        session_id: ctx.session_id,
        toll_id: ctx.toll_id,
        etag_id: ctx.etag_id.clone(),
        lane_id: ctx.lane_id,
        plate: ctx.plate.clone(),
        request_received_ms: ctx.request_received_ms,
        response_ticket_id: ctx.response_ticket_id,
        tcoc_request_id: ctx.tcoc_request_id,
    };
    let response_clone = if response.is_empty() {
        Vec::new()
    } else {
        response.to_vec()
    };
    let direction_clone = direction.clone();
    let station_in_for_out_clone = station_in_for_out;

    // Background: update TCOC_REQUEST toll_in/toll_out (same as ROAMING) and save TCOC_RESPONSE without blocking response.
    tokio::spawn(async move {
        if let Err(e) = TcocRequestService::new()
            .update_toll_in_toll_out_by_request_id(
                request_id,
                direction_clone,
                station_in_for_out_clone,
                toll_id,
                ctx_clone.tcoc_request_id,
            )
            .await
        {
            tracing::warn!(request_id, error = %e, "[Processor] TCOC_REQUEST update toll_in/toll_out failed");
        }
    });
    save_tcoc_response_if_applicable(
        &ctx_clone,
        response_command_id,
        &response_clone,
        Some(status),
        response_ticket_id,
        direction,
        station_in_for_out,
        response_send_ms,
    );
}

/// Route request to handler by command_id; encryption_key from TCOC_CONNECTION_SERVER.
/// client_ip: client IP address (FE/gate), used for CONNECT to save TCOC_SESSIONS.IP_ADDRESS.
#[allow(clippy::too_many_arguments)]
pub async fn process_request(
    data: Vec<u8>,
    conn_id: i32,
    command_id: i32,
    cache: Arc<CacheManager>,
    db_toll_id: Option<String>,
    tx_session_updates: SessionUpdateSender,
    encryption_key: &str,
    client_ip: Option<String>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    if !FE_VALID_COMMANDS.contains(&command_id) {
        let request_id = fe_protocol::request_id_from_decrypted(&data, command_id);
        let session_id = if data.len() >= 24 {
            i64::from_le_bytes(data[16..24].try_into().unwrap())
        } else {
            0
        };
        tracing::error!(conn_id, request_id, command_hex = %format!("0x{:02X}", command_id), "[Processor] invalid command ID");
        let fe_request_err = FE_REQUEST {
            message_length: if data.len() >= 4 {
                i32::from_le_bytes(data[0..4].try_into().unwrap())
            } else {
                0
            },
            command_id,
            request_id,
            session_id,
        };
        let req_service = TcocRequestService::new();
        let tcoc_request_id_err = req_service
            .save_request(
                &fe_request_err,
                conn_id,
                &data,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .ok();
        if tcoc_request_id_err.is_none() {
            tracing::warn!(conn_id, request_id, command_id = %format!("0x{:02X}", command_id), "[Processor] TCOC_REQUEST save failed (invalid command)");
        }
        let request_received_ms = crate::utils::timestamp_ms();
        let ctx_err = RequestContext {
            conn_id,
            request_id,
            session_id,
            toll_id: None,
            etag_id: None,
            lane_id: None,
            plate: None,
            request_received_ms,
            response_ticket_id: None,
            tcoc_request_id: tcoc_request_id_err,
        };
        // Only save RESPONSE when we have a REQUEST (keep 1:1 pair).
        if tcoc_request_id_err.is_some() {
            save_tcoc_response_if_applicable(
                &ctx_err,
                fe_response_command_id(command_id),
                &[],
                Some(fe::STATUS_COMMAND_ID_INVALID),
                None,
                None,
                None,
                crate::utils::timestamp_ms(),
            );
        }
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Invalid command ID: 0x{:02X}", command_id),
        )));
    }

    if data.len() < 8 {
        tracing::error!(
            conn_id,
            data_len = data.len(),
            "[Processor] request too short for header"
        );
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "FE request too short for header",
        )));
    }

    let min_len = fe_protocol::min_len_for_command(command_id);
    if data.len() < min_len {
        tracing::error!(
            conn_id,
            data_len = data.len(),
            min_len,
            command_id = %format!("0x{:02X}", command_id),
            "[Processor] request shorter than min message length"
        );
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "FE request too short: {} bytes (min {} for command 0x{:02X})",
                data.len(),
                min_len,
                command_id
            ),
        )));
    }

    let message_length = i32::from_le_bytes(data[0..4].try_into().unwrap());
    let _message_len = if message_length <= 0 || message_length as usize > data.len() {
        data.len()
    } else {
        message_length as usize
    };

    let (req_id, sess_id) = if command_id == fe::HANDSHAKE {
        match fe_protocol::parse_shake_ids(&data) {
            Some((_, r, s)) => (r, s),
            None => {
                tracing::error!(conn_id, data_len = data.len(), command_id = %format!("0x{:02X}", command_id), "[Processor] parse SHAKE ids failed");
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "FE SHAKE message too short for header ids (need 28 bytes)",
                )));
            }
        }
    } else if command_id == fe::CONNECT {
        let r = fe_protocol::request_id_from_decrypted(&data, command_id);
        (r, 0)
    } else if command_id == fe::TERMINATE {
        match fe_protocol::parse_terminate_ids(&data) {
            Some((_, r, s)) => (r, s),
            None => {
                tracing::error!(conn_id, data_len = data.len(), command_id = %format!("0x{:02X}", command_id), "[Processor] parse TERMINATE ids failed");
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "FE TERMINATE message too short for header ids (need 28 bytes)",
                )));
            }
        }
    } else {
        match fe_protocol::parse_request_id_session_id(&data) {
            Some(pair) => pair,
            None => {
                tracing::error!(conn_id, data_len = data.len(), command_id = %format!("0x{:02X}", command_id), "[Processor] parse request_id/session_id failed");
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "FE request too short for header ids",
                )));
            }
        }
    };
    let fe_request = FE_REQUEST {
        message_length,
        command_id: i32::from_le_bytes(data[4..8].try_into().unwrap()),
        request_id: req_id,
        session_id: sess_id,
    };

    // Capture request-received time as early as possible so TCOC process_duration reflects total processing (parse + handler).
    let request_received_ms = crate::utils::timestamp_ms();

    let off = fe_protocol::fe_body_offsets(command_id);
    let toll_id = if command_id == fe::CONNECT
        || command_id == fe::CHECKIN
        || command_id == fe::COMMIT
        || command_id == fe::ROLLBACK
        || command_id == fe::CHECKOUT_RESERVE_BOO
        || command_id == fe::CHECKOUT_COMMIT_BOO
        || command_id == fe::CHECKOUT_ROLLBACK_BOO
    {
        if off.toll.1 > off.toll.0 && data.len() >= off.toll.1 {
            Some(i32::from_le_bytes(data[off.toll.0..off.toll.1].try_into().unwrap()) as i64)
        } else if command_id == fe::CONNECT {
            None
        } else {
            tracing::warn!(
                conn_id,
                request_id = fe_request.request_id,
                data_len = data.len(),
                "[Processor] request data short for toll_id"
            );
            None
        }
    } else {
        None
    };

    if let Some(request_toll_id) = toll_id {
        if let Some(ref db_toll_id_str) = db_toll_id {
            if !db_toll_id_str.trim().is_empty() && !is_toll_id_valid(request_toll_id, &db_toll_id)
            {
                tracing::error!(conn_id, request_id = fe_request.request_id, request_toll_id, db_toll_id = ?db_toll_id, "[Processor] toll_id not in allowed list (status {})", fe::NOT_FOUND_STATION_LANE);
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "Error {}: toll_id {} not found in allowed list",
                        fe::NOT_FOUND_STATION_LANE,
                        request_toll_id
                    ),
                )));
            }
        }
    }

    let etag_id = if off.etag.1 > off.etag.0 && data.len() >= off.etag.1 {
        Some(
            String::from_utf8_lossy(&data[off.etag.0..off.etag.1])
                .trim_end_matches('\0')
                .to_string(),
        )
    } else {
        None
    };

    let lane_id = if off.lane.1 > off.lane.0 && data.len() >= off.lane.1 {
        Some(i32::from_le_bytes(data[off.lane.0..off.lane.1].try_into().unwrap()) as i64)
    } else {
        None
    };

    let plate = if off.plate.1 > off.plate.0 && data.len() >= off.plate.1 {
        Some(
            String::from_utf8_lossy(&data[off.plate.0..off.plate.1])
                .trim_end_matches('\0')
                .to_string(),
        )
    } else {
        None
    };

    let tcoc_tid =
        fe_protocol::parse_ticket_id_commit_rollback(&data, command_id).map(|i| i.to_string());

    // request_received_ms already set above (right after fe_request) for accurate TCOC process_duration.

    // TCOC: save REQUEST first (await), then dispatch handler; RESPONSE and update toll_in/toll_out run in background (spawn) for throughput.
    let service = TcocRequestService::new();
    let tcoc_request_id = service
        .save_request(
            &fe_request,
            conn_id,
            &data,
            toll_id,
            etag_id.clone(),
            lane_id,
            plate.clone(),
            tcoc_tid,
            None,
        )
        .await
        .ok();
    if tcoc_request_id.is_none() {
        tracing::warn!(conn_id, request_id = fe_request.request_id, command_id = %format!("0x{:02X}", command_id), "[Processor] TCOC_REQUEST save failed");
    }

    if command_id != fe::CONNECT {
        let session_id = fe_request.session_id;
        let _ = tx_session_updates.send(SessionUpdate::UpdateHandshake {
            session_id,
            last_received_time: Instant::now(),
        });
    }

    let response_ticket_id = fe_protocol::parse_ticket_id_commit_rollback(&data, command_id);

    let ctx = RequestContext {
        conn_id,
        request_id: fe_request.request_id,
        session_id: fe_request.session_id,
        toll_id,
        etag_id,
        lane_id,
        plate,
        request_received_ms,
        response_ticket_id,
        tcoc_request_id,
    };

    tracing::debug!(conn_id = ctx.conn_id, request_id = ctx.request_id, command_id = %format!("0x{:02X}", command_id), "[Processor] routing to handler");
    // command_id → handler: CONNECT→handle_connect, HANDSHAKE→handle_handshake, CHECKIN→handle_checkin, COMMIT→handle_commit, ROLLBACK→handle_rollback, TERMINATE→handle_terminate
    Ok(match command_id {
        fe::CONNECT => {
            tracing::debug!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                "[Processor] CONNECT"
            );
            let response = handle_connect(
                fe_request,
                data,
                conn_id,
                tx_session_updates.clone(),
                encryption_key,
                client_ip.as_deref(),
            )
            .await?;
            let response_send_ms = crate::utils::timestamp_ms();
            save_tcoc_response_if_applicable(
                &ctx,
                fe::CONNECT_RESP,
                &response,
                Some(0),
                None,
                None,
                None,
                response_send_ms,
            );
            response
        }
        fe::HANDSHAKE => {
            let response =
                handle_handshake(fe_request, data, conn_id, command_id, encryption_key).await?;
            let response_send_ms = crate::utils::timestamp_ms();
            save_tcoc_response_if_applicable(
                &ctx,
                fe::SHAKE_RESP,
                &response,
                Some(0),
                None,
                None,
                None,
                response_send_ms,
            );
            response
        }
        fe::CHECKIN => {
            tracing::debug!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                "[Processor] CHECKIN"
            );
            let (response, status, _called_boo_client, direction, station_in_for_out) =
                handle_checkin(
                    fe_request,
                    &data,
                    conn_id,
                    encryption_key,
                    RATING_DB.clone(),
                    cache.clone(),
                )
                .await?;
            let response_send_ms = crate::utils::timestamp_ms();
            let ids = crate::utils::log_trace_ids(
                ctx.request_id,
                ctx.response_ticket_id,
                ctx.etag_id.as_deref(),
            );
            tracing::info!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                etag = ?ctx.etag_id,
                fe_status = status,
                "{} [Processor] CHECKIN finished",
                ids
            );
            apply_post_handler_tcoc_update(
                &ctx,
                fe::CHECKIN_RESP,
                &response,
                status,
                direction,
                station_in_for_out,
                None,
                response_send_ms,
            );
            response
        }
        fe::COMMIT => {
            tracing::debug!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                "[Processor] COMMIT"
            );
            let (response, status, _called_boo_client, direction, station_in_for_out) =
                handle_commit(
                    fe_request,
                    data,
                    conn_id,
                    command_id,
                    encryption_key,
                    RATING_DB.clone(),
                    cache.clone(),
                )
                .await?;
            let response_send_ms = crate::utils::timestamp_ms();
            let ids = crate::utils::log_trace_ids(
                ctx.request_id,
                ctx.response_ticket_id,
                ctx.etag_id.as_deref(),
            );
            tracing::info!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                ticket_id = ctx.response_ticket_id,
                etag = ?ctx.etag_id,
                fe_status = status,
                "{} [Processor] COMMIT finished",
                ids
            );
            apply_post_handler_tcoc_update(
                &ctx,
                fe::COMMIT_RESP,
                &response,
                status,
                direction,
                station_in_for_out,
                ctx.response_ticket_id,
                response_send_ms,
            );
            response
        }
        fe::ROLLBACK => {
            tracing::debug!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                "[Processor] ROLLBACK"
            );
            let (response, status, _called_boo_client, direction, station_in_for_out) =
                handle_rollback(
                    fe_request,
                    data,
                    conn_id,
                    command_id,
                    encryption_key,
                    Some(RATING_DB.clone()),
                    Some(cache.clone()),
                )
                .await?;
            let response_send_ms = crate::utils::timestamp_ms();
            let ids = crate::utils::log_trace_ids(
                ctx.request_id,
                ctx.response_ticket_id,
                ctx.etag_id.as_deref(),
            );
            tracing::info!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                ticket_id = ctx.response_ticket_id,
                etag = ?ctx.etag_id,
                fe_status = status,
                "{} [Processor] ROLLBACK finished",
                ids
            );
            apply_post_handler_tcoc_update(
                &ctx,
                fe::ROLLBACK_RESP,
                &response,
                status,
                direction,
                station_in_for_out,
                ctx.response_ticket_id,
                response_send_ms,
            );
            response
        }
        fe::TERMINATE => {
            tracing::debug!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                "[Processor] TERMINATE"
            );
            let (response, status) = handle_terminate(
                fe_request,
                data,
                conn_id,
                tx_session_updates.clone(),
                encryption_key,
            )
            .await?;
            let response_send_ms = crate::utils::timestamp_ms();
            save_tcoc_response_if_applicable(
                &ctx,
                fe::TERMINATE_RESP,
                &response,
                Some(status),
                None,
                None,
                None,
                response_send_ms,
            );
            response
        }
        fe::QUERY_VEHICLE_BOO => {
            tracing::debug!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                "[Processor] QUERY_VEHICLE_BOO"
            );
            let (response, status) = handle_query_vehicle_boo(
                fe_request,
                &data,
                conn_id,
                encryption_key,
                RATING_DB.clone(),
            )
            .await?;
            let response_send_ms = crate::utils::timestamp_ms();
            save_tcoc_response_if_applicable(
                &ctx,
                fe::QUERY_VEHICLE_BOO_RESP,
                &response,
                Some(status),
                None,
                None,
                None,
                response_send_ms,
            );
            response
        }
        fe::LOOKUP_VEHICLE => {
            tracing::debug!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                "[Processor] LOOKUP_VEHICLE"
            );
            let (response, status) = handle_lookup_vehicle(
                fe_request,
                &data,
                conn_id,
                encryption_key,
                RATING_DB.clone(),
            )
            .await?;
            let response_send_ms = crate::utils::timestamp_ms();
            save_tcoc_response_if_applicable(
                &ctx,
                fe::LOOKUP_VEHICLE_RESP,
                &response,
                Some(status),
                None,
                None,
                None,
                response_send_ms,
            );
            response
        }
        fe::CHECKOUT_RESERVE_BOO => {
            tracing::debug!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                "[Processor] CHECKOUT_RESERVE_BOO"
            );
            let (response, status) = handle_checkout_reserve_boo(
                fe_request,
                &data,
                conn_id,
                encryption_key,
                RATING_DB.clone(),
                cache.clone(),
            )
            .await?;
            let response_send_ms = crate::utils::timestamp_ms();
            let ids = crate::utils::log_trace_ids(
                ctx.request_id,
                ctx.response_ticket_id,
                ctx.etag_id.as_deref(),
            );
            tracing::info!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                etag = ?ctx.etag_id,
                fe_status = status,
                "{} [Processor] CHECKOUT_RESERVE_BOO finished",
                ids
            );
            save_tcoc_response_if_applicable(
                &ctx,
                fe::CHECKOUT_RESERVE_BOO_RESP,
                &response,
                Some(status),
                None,
                None,
                None,
                response_send_ms,
            );
            response
        }
        fe::CHECKOUT_COMMIT_BOO => {
            tracing::debug!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                "[Processor] CHECKOUT_COMMIT_BOO"
            );
            let (response, status) = handle_checkout_commit_boo(
                fe_request,
                &data,
                conn_id,
                encryption_key,
                RATING_DB.clone(),
                cache.clone(),
            )
            .await?;
            let response_send_ms = crate::utils::timestamp_ms();
            let ids = crate::utils::log_trace_ids(
                ctx.request_id,
                ctx.response_ticket_id,
                ctx.etag_id.as_deref(),
            );
            tracing::info!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                etag = ?ctx.etag_id,
                fe_status = status,
                "{} [Processor] CHECKOUT_COMMIT_BOO finished",
                ids
            );
            save_tcoc_response_if_applicable(
                &ctx,
                fe::CHECKOUT_COMMIT_BOO_RESP,
                &response,
                Some(status),
                None,
                None,
                None,
                response_send_ms,
            );
            response
        }
        fe::CHECKOUT_ROLLBACK_BOO => {
            tracing::debug!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                "[Processor] CHECKOUT_ROLLBACK_BOO"
            );
            let (response, status) =
                handle_checkout_rollback_boo(fe_request, &data, conn_id, encryption_key).await?;
            let response_send_ms = crate::utils::timestamp_ms();
            let ids = crate::utils::log_trace_ids(
                ctx.request_id,
                ctx.response_ticket_id,
                ctx.etag_id.as_deref(),
            );
            tracing::info!(
                conn_id = ctx.conn_id,
                request_id = ctx.request_id,
                etag = ?ctx.etag_id,
                fe_status = status,
                "{} [Processor] CHECKOUT_ROLLBACK_BOO finished",
                ids
            );
            save_tcoc_response_if_applicable(
                &ctx,
                fe::CHECKOUT_ROLLBACK_BOO_RESP,
                &response,
                Some(status),
                None,
                None,
                None,
                response_send_ms,
            );
            response
        }
        _ => {
            tracing::error!(conn_id = ctx.conn_id, request_id = ctx.request_id, command_id = %format!("0x{:02X}", command_id), "[Processor] unhandled command");
            let response_send_ms = crate::utils::timestamp_ms();
            save_tcoc_response_if_applicable(
                &ctx,
                fe_response_command_id(command_id),
                &[],
                Some(fe::STATUS_COMMAND_ID_INVALID),
                None,
                None,
                None,
                response_send_ms,
            );
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Unhandled command ID: 0x{:02X}", command_id),
            )));
        }
    })
}
