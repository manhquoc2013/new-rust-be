//! Handler ROLLBACK: FE gửi ROLLBACK (req), backend trả ROLLBACK_RESP (resp).
//! Cặp msg req/resp từ FE: FE gửi ROLLBACK (3A, 0x6A) → processor gọi handle_rollback → handler trả ROLLBACK_RESP (3B, 0x6B) cho FE.

mod handler;
mod common;

use crate::cache::config::cache_manager::CacheManager;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::crypto::create_encryptor_with_key;
use crate::fe_protocol;
use crate::handlers::roaming;
use crate::models::TCOCmessages::{FE_REQUEST, FE_ROLLBACK};
use r2d2::Pool;
use std::error::Error;
use std::sync::Arc;

/// Xử lý ROLLBACK (FE req 3A/0x6A): parse FE_ROLLBACK, gọi process handler, trả ROLLBACK_RESP (resp 3B/0x6B) cho FE.
/// Trả về (response, status, called_boo_client, direction, station_in_for_out). direction lấy từ lane khi có db_pool + cache.
#[allow(clippy::too_many_arguments)]
pub async fn handle_rollback(
    rq: FE_REQUEST,
    data: Vec<u8>,
    conn_id: i32,
    _command_id: i32,
    encryption_key: &str,
    db_pool: Option<Arc<Pool<OdbcConnectionManager>>>,
    cache: Option<Arc<CacheManager>>,
) -> Result<(Vec<u8>, i32, bool, Option<String>, Option<i64>), Box<dyn Error>> {
    let encryptor = create_encryptor_with_key(encryption_key);

    if data.len() < fe_protocol::len::ROLLBACK {
        return Err(format!(
            "ROLLBACK message too short: {} bytes (minimum {})",
            data.len(),
            fe_protocol::len::ROLLBACK
        )
        .into());
    }

    let mut fe_rollback: FE_ROLLBACK = FE_ROLLBACK::default();
    fe_rollback.message_length = rq.message_length;
    fe_rollback.command_id = rq.command_id;
    fe_rollback.request_id = i64::from_le_bytes(data[8..16].try_into().unwrap());
    fe_rollback.session_id = i64::from_le_bytes(data[16..24].try_into().unwrap());
    fe_rollback.etag = String::from_utf8_lossy(&data[24..48]).to_string();
    fe_rollback.station = i32::from_le_bytes(data[48..52].try_into().unwrap());
    fe_rollback.lane = i32::from_le_bytes(data[52..56].try_into().unwrap());
    fe_rollback.ticket_id = i64::from_le_bytes(data[56..64].try_into().unwrap());
    fe_rollback.status = i32::from_le_bytes(data[64..68].try_into().unwrap());
    fe_rollback.plate = String::from_utf8_lossy(&data[68..78]).to_string();
    fe_rollback.image_count = i32::from_le_bytes(data[78..82].try_into().unwrap());
    fe_rollback.weight = i32::from_le_bytes(data[82..86].try_into().unwrap());
    fe_rollback.reason_id = i32::from_le_bytes(data[86..90].try_into().unwrap());

    tracing::debug!(
        conn_id,
        request_id = fe_rollback.request_id,
        "[Rollback] FE_ROLLBACK parsed"
    );

    let direction = match (db_pool.as_ref(), cache.as_ref()) {
        (Some(p), Some(c)) => {
            roaming::get_direction_from_lane(
                Some(fe_rollback.station as i64),
                Some(fe_rollback.lane as i64),
                p.clone(),
                c.clone(),
            )
            .await
        }
        _ => None,
    };

    handler::process_rollback(&fe_rollback, conn_id, &encryptor)
        .await
        .map(|(r, s)| (r, s, false, direction, None))
}
