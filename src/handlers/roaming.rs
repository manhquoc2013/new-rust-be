//! Roaming/direction helpers: resolve lane direction (I/O) from station+lane for TCOC.

use crate::cache::config::cache_manager::CacheManager;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::constants::lane_type;
use crate::models::TollCache::get_toll_lanes_by_toll_id_with_fallback;
use r2d2::Pool;
use std::sync::Arc;

/// Resolve direction "I" (in) or "O" (out) from station (toll_id) and lane (lane_code).
/// Uses TOLL_LANE cache/DB; returns None if station/lane missing or lane_type not IN/OUT.
pub async fn get_direction_from_lane(
    station_id: Option<i64>,
    lane_id: Option<i64>,
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
) -> Option<String> {
    let toll_id = station_id.and_then(|s| i32::try_from(s).ok())?;
    let lane_code = lane_id.and_then(|l| i32::try_from(l).ok())?;

    let lanes = get_toll_lanes_by_toll_id_with_fallback(toll_id, db_pool, cache).await;
    let lane = lanes.iter().find(|l| l.lane_code == lane_code)?;
    match lane.lane_type.trim() {
        lane_type::IN => Some("I".to_string()),
        lane_type::OUT => Some("O".to_string()),
        _ => None,
    }
}
