//! Handler CHECKIN: phân loại theo eTag, gọi VETC/VDTC/BECT.

mod bect;
pub(crate) mod common;

use crate::cache::config::cache_manager::CacheManager;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::constants::{fe, lane_type};
use crate::crypto::create_encryptor_with_key;
use crate::fe_protocol;
use crate::models::TCOCmessages::{FE_CHECKIN, FE_CHECKIN_IN_RESP, FE_REQUEST};
use crate::models::TollCache::{get_toll_lanes_by_toll_id_with_fallback, get_toll_with_fallback};
use aes;
use cbc;
use r2d2::Pool;
use std::error::Error;
use std::sync::Arc;

use common::{serialize_and_encrypt_response, PLATE_EMPTY_SENTINEL};

/// Tạo response lỗi NOT_FOUND_STATION_LANE cho CHECKIN (toll/lane không tồn tại hoặc lane_type không hợp lệ).
async fn build_checkin_not_found_response(
    fe_checkin: &FE_CHECKIN,
    conn_id: i32,
    toll_id_for_direction: i64,
    encryptor: &cbc::Encryptor<aes::Aes128>,
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
) -> Result<(Vec<u8>, i32, Option<String>), Box<dyn Error>> {
    let direction = crate::handlers::roaming::get_direction_from_lane(
        Some(toll_id_for_direction),
        Some(fe_checkin.lane as i64),
        db_pool,
        cache,
    )
    .await;
    let mut fe_resp: FE_CHECKIN_IN_RESP = FE_CHECKIN_IN_RESP::default();
    fe_resp.message_length = fe_protocol::response_checkin_in_resp_len();
    fe_resp.command_id = fe::CHECKIN_RESP;
    fe_resp.request_id = fe_checkin.request_id;
    fe_resp.session_id = conn_id as i64;
    fe_resp.status = fe::NOT_FOUND_STATION_LANE;
    fe_resp.etag = fe_checkin.etag.clone();
    fe_resp.station = fe_checkin.station;
    fe_resp.lane = fe_checkin.lane;
    fe_resp.plate = fe_checkin.plate.trim().to_string();
    if fe_resp.plate.is_empty() {
        fe_resp.plate = PLATE_EMPTY_SENTINEL.to_string();
    }
    let reply_bytes = serialize_and_encrypt_response(&fe_resp, encryptor).await?;
    Ok((reply_bytes, fe_resp.status, direction))
}

/// Xử lý CHECKIN command: parse request, lấy toll/lane, gọi handler BECT.
pub async fn handle_checkin(
    rq: FE_REQUEST,
    data: &[u8],
    conn_id: i32,
    encryption_key: &str,
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
) -> Result<(Vec<u8>, i32, bool, Option<String>, Option<i64>), Box<dyn Error>> {
    let encryptor = create_encryptor_with_key(encryption_key);
    if data.len() < fe_protocol::len::CHECKIN {
        return Err(format!(
            "CHECKIN message too short: {} bytes (minimum {})",
            data.len(),
            fe_protocol::len::CHECKIN
        )
        .into());
    }

    let mut fe_checkin: FE_CHECKIN = FE_CHECKIN::default();
    fe_checkin.message_length = rq.message_length;
    fe_checkin.command_id = rq.command_id;
    fe_checkin.request_id = i64::from_le_bytes(data[8..16].try_into().unwrap());
    fe_checkin.session_id = i64::from_le_bytes(data[16..24].try_into().unwrap());
    fe_checkin.etag = String::from_utf8_lossy(&data[24..48]).to_string();
    fe_checkin.station = i32::from_le_bytes(data[48..52].try_into().unwrap());
    fe_checkin.lane = i32::from_le_bytes(data[52..56].try_into().unwrap());
    fe_checkin.plate = String::from_utf8_lossy(&data[56..66]).to_string();
    fe_checkin.tid = String::from_utf8_lossy(&data[66..90]).to_string();
    fe_checkin.hash_value = String::from_utf8_lossy(&data[90..106]).to_string();

    tracing::debug!(request_id = fe_checkin.request_id, "[Processor] FE_CHECKIN");

    let _toll = match get_toll_with_fallback(fe_checkin.station, db_pool.clone(), cache.clone())
        .await
    {
        Some(t) => {
            tracing::debug!(request_id = fe_checkin.request_id, toll_id = t.toll_id, toll_name = %t.toll_name, "[Processor] TOLL cache hit");
            t
        }
        None => {
            tracing::error!(
                conn_id,
                request_id = fe_checkin.request_id,
                station = fe_checkin.station,
                "[Processor] TOLL not found"
            );
            let (reply_bytes, status, direction) = build_checkin_not_found_response(
                &fe_checkin,
                conn_id,
                fe_checkin.station as i64,
                &encryptor,
                db_pool.clone(),
                cache.clone(),
            )
            .await?;
            return Ok((reply_bytes, status, false, direction, None));
        }
    };

    let lanes =
        get_toll_lanes_by_toll_id_with_fallback(fe_checkin.station, db_pool.clone(), cache.clone())
            .await;
    let toll_lane = match lanes.iter().find(|l| l.lane_code == fe_checkin.lane) {
        Some(l) => {
            tracing::debug!(conn_id, request_id = fe_checkin.request_id, toll_lane_id = l.toll_lane_id, lane_name = %l.lane_name, "[Processor] TOLL_LANE cache hit");
            l.clone()
        }
        None => {
            tracing::error!(
                conn_id,
                request_id = fe_checkin.request_id,
                station = fe_checkin.station,
                lane = fe_checkin.lane,
                "[Processor] TOLL_LANE not found"
            );
            let (reply_bytes, status, direction) = build_checkin_not_found_response(
                &fe_checkin,
                conn_id,
                _toll.toll_id as i64,
                &encryptor,
                db_pool.clone(),
                cache.clone(),
            )
            .await?;
            return Ok((reply_bytes, status, false, direction, None));
        }
    };

    let lane_type_str = &toll_lane.lane_type;
    match lane_type_str.as_str() {
        lane_type::IN => {}
        lane_type::OUT => {}
        _ => {
            tracing::error!(conn_id, request_id = fe_checkin.request_id, lane_type = %lane_type_str, "[Processor] Invalid lane_type (expected IN/OUT)");
            let (reply_bytes, status, direction) = build_checkin_not_found_response(
                &fe_checkin,
                conn_id,
                _toll.toll_id as i64,
                &encryptor,
                db_pool.clone(),
                cache.clone(),
            )
            .await?;
            return Ok((reply_bytes, status, false, direction, None));
        }
    }

    let direction = crate::handlers::roaming::get_direction_from_lane(
        Some(_toll.toll_id as i64),
        Some(fe_checkin.lane as i64),
        db_pool.clone(),
        cache.clone(),
    )
    .await;

    tracing::debug!(
        request_id = fe_checkin.request_id,
        "[Processor] CHECKIN BOO type BECT"
    );
    bect::handle_bect_checkin(
        &fe_checkin,
        conn_id,
        &_toll,
        &toll_lane,
        &encryptor,
        db_pool.clone(),
        cache.clone(),
    )
    .await
    .map(|(r, s)| (r, s, false, direction.clone(), None))
}
