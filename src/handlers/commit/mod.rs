//! Handler COMMIT: cặp msg req/resp từ FE. FE gửi COMMIT (req, 3A/0x68) → processor gọi handle_commit → handler trả COMMIT_RESP (resp, 3B/0x69) cho FE.

mod handler;
pub(crate) mod common;
pub(crate) mod kafka_payload;

use crate::cache::config::cache_manager::CacheManager;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::crypto::create_encryptor_with_key;
use crate::fe_protocol;
use crate::logging::process_type::ProcessTypeGuard;
use crate::logging::ProcessType;
use crate::models::TCOCmessages::{FE_COMMIT_IN, FE_REQUEST};
use r2d2::Pool;
use std::error::Error;
use std::sync::Arc;

/// Handles COMMIT (FE req 3A/0x68): parse FE_COMMIT_IN, gọi process handler, return COMMIT_RESP (resp 3B/0x69) to FE.
/// Returns (response, status, called_boo_client, direction, station_in_for_out) for TCOC in/out.
pub async fn handle_commit(
    rq: FE_REQUEST,
    data: Vec<u8>,
    conn_id: i32,
    _command_id: i32,
    encryption_key: &str,
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
) -> Result<(Vec<u8>, i32, bool, Option<String>, Option<i64>), Box<dyn Error>> {
    let encryptor = create_encryptor_with_key(encryption_key);

    if data.len() < fe_protocol::len::COMMIT {
        return Err(format!(
            "COMMIT message too short: {} bytes (minimum {})",
            data.len(),
            fe_protocol::len::COMMIT
        )
        .into());
    }

    let mut fe_commit: FE_COMMIT_IN = FE_COMMIT_IN::default();
    fe_commit.message_length = rq.message_length;
    fe_commit.command_id = rq.command_id;
    fe_commit.request_id = i64::from_le_bytes(data[8..16].try_into().unwrap());
    fe_commit.session_id = i64::from_le_bytes(data[16..24].try_into().unwrap());
    fe_commit.etag = String::from_utf8_lossy(&data[24..48]).to_string();
    fe_commit.station = i32::from_le_bytes(data[48..52].try_into().unwrap());
    fe_commit.lane = i32::from_le_bytes(data[52..56].try_into().unwrap());
    fe_commit.ticket_id = i64::from_le_bytes(data[56..64].try_into().unwrap());
    fe_commit.status = i32::from_le_bytes(data[64..68].try_into().unwrap());
    fe_commit.plate = String::from_utf8_lossy(&data[68..78]).to_string();
    fe_commit.image_count = i32::from_le_bytes(data[78..82].try_into().unwrap());
    fe_commit.vehicle_length = i32::from_le_bytes(data[82..86].try_into().unwrap());
    fe_commit.transaction_amount = i32::from_le_bytes(data[86..90].try_into().unwrap());
    fe_commit.weight = i32::from_le_bytes(data[90..94].try_into().unwrap());
    fe_commit.reason_id = i32::from_le_bytes(data[94..98].try_into().unwrap());

    tracing::debug!(
        request_id = fe_commit.request_id,
        "[Processor] FE_COMMIT_IN decrypted"
    );

    let direction = crate::handlers::roaming::get_direction_from_lane(
        Some(fe_commit.station as i64),
        Some(fe_commit.lane as i64),
        db_pool.clone(),
        cache.clone(),
    )
    .await;

    let _process_type_guard = ProcessTypeGuard::new(ProcessType::BETC);

    tracing::debug!(request_id = fe_commit.request_id, "[Processor] COMMIT");
    handler::process_commit(&fe_commit, conn_id, &encryptor, db_pool, cache)
        .await
        .map(|(r, s)| (r, s, false, direction, None))
}
