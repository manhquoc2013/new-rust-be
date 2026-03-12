//! Cache làn thu phí (toll_lane), get_toll_lane_cache, load from DB.

use std::sync::Arc;

use crate::{
    cache::{
        config::{cache_manager::CacheManager, cache_prefix::CachePrefix},
        data::dto::toll_lane_dto::TollLaneDto,
    },
    configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager},
    db::{get_i32, get_nullable_i32, get_nullable_string, DbError},
    models::TollCache::{replace_all_toll_lanes, save_toll_lane},
};
use odbc_api::Cursor;
use r2d2::Pool;
use tokio::task;

/// Load cache TollLane. Prefer DB; khi startup nếu DB failed hoặc pool None thì fallback KeyDB (khi `load_from_keydb` true). Reload luôn từ DB.
/// `pool_opt`: Option vì RATING có thể chưa sẵn sàng lúc khởi động; khi None to omit DB.
pub async fn get_toll_lane_cache(
    pool_opt: Option<Arc<Pool<OdbcConnectionManager>>>,
    cache: Arc<CacheManager>,
    load_from_keydb: bool,
) {
    let result = match pool_opt {
        Some(pool) => get_active_toll_lanes(pool).await,
        None => {
            tracing::warn!(
                "[Cache] TollLane skip DB (pool not available), will use KeyDB fallback if enabled"
            );
            Err(crate::db::error::DbError::ExecutionError(
                "DB pool not available".to_string(),
            ))
        }
    };
    match result {
        Ok(lanes) => {
            let cache_data: Vec<(String, TollLaneDto)> = lanes
                .iter()
                .map(|lane_dto| {
                    let key = cache.gen_key(
                        CachePrefix::TollLane,
                        &[&lane_dto.toll_id, &lane_dto.lane_code],
                    );
                    (key, lane_dto.clone())
                })
                .collect();
            cache
                .atomic_reload_prefix(CachePrefix::TollLane.as_str(), cache_data.clone())
                .await;
            let in_memory_lanes: Vec<_> = lanes.into_iter().map(|dto| dto.to_toll_lane()).collect();
            let lane_count = in_memory_lanes.len();
            replace_all_toll_lanes(in_memory_lanes);
            if load_from_keydb {
                tracing::info!(
                    lane_count,
                    "[Cache] TollLane loaded from DB, synced to KeyDB"
                );
            } else {
                tracing::info!(lane_count, "[Cache] TollLane loaded from DB");
            }
        }
        Err(e) => {
            tracing::error!(error = ?e, "[Cache] TollLane load from DB failed");
            if !load_from_keydb {
                return;
            }
            let prefix = CachePrefix::TollLane.as_str();
            let keydb_loaded = cache.load_prefix_from_keydb::<TollLaneDto>(prefix).await;
            if keydb_loaded.is_empty() {
                tracing::warn!("[Cache] TollLane fallback KeyDB empty, keeping empty");
                return;
            }
            cache
                .atomic_reload_prefix(prefix, keydb_loaded.clone())
                .await;
            let all_dtos: Vec<TollLaneDto> = keydb_loaded.into_iter().map(|(_, d)| d).collect();
            let in_memory_lanes: Vec<_> = all_dtos.iter().map(|dto| dto.to_toll_lane()).collect();
            replace_all_toll_lanes(in_memory_lanes);
            tracing::info!(
                lane_count = all_dtos.len(),
                "[Cache] TollLane loaded from KeyDB (DB failed, fallback)"
            );
        }
    }
}

/// Lấy toll_lane từ cache, nếu không có thì query DB và cache lại (cache-aside pattern)
#[allow(dead_code)]
pub async fn get_toll_lane(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
    toll_id: i32,
    lane_code: i32,
) -> Option<TollLaneDto> {
    // Bước 1: Thử lấy từ cache
    let key = cache.gen_key(CachePrefix::TollLane, &[&toll_id, &lane_code]);

    if let Some(lane) = cache.get::<TollLaneDto>(&key).await {
        tracing::debug!(toll_id, lane_code, "[Cache] TollLane cache hit");
        // Cũng load vào in-memory cache
        let toll_lane = lane.to_toll_lane();
        save_toll_lane(toll_lane);
        return Some(lane);
    }

    tracing::debug!(toll_id, lane_code, "[Cache] TollLane cache miss");

    match get_toll_lane_from_db(db_pool.clone(), toll_id, lane_code).await {
        Ok(Some(lane)) => {
            // Bước 3: Lưu vào cache để lần sau dùng
            cache.set(&key, &lane).await;
            // Cũng load vào in-memory cache
            let toll_lane = lane.to_toll_lane();
            save_toll_lane(toll_lane);
            tracing::debug!(toll_id, lane_code, "[Cache] TollLane loaded from DB");
            Some(lane)
        }
        Ok(None) => {
            tracing::warn!(toll_id, lane_code, "[Cache] TollLane not found in DB");
            None
        }
        Err(e) => {
            tracing::error!(toll_id, lane_code, error = ?e, "[Cache] TollLane DB query failed");
            None
        }
    }
}

/// Lấy tất cả toll_lanes của một toll_id từ cache, nếu không có thì query DB và cache lại
pub async fn get_toll_lanes_by_toll_id(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
    toll_id: i32,
) -> Vec<TollLaneDto> {
    // Bước 1: Thử lấy từ cache (lấy từng lane từ in-memory cache)
    // Note: Có thể optimize bằng cách cache cả list, nhưng hiện tại cache từng item
    use crate::models::TollCache::get_toll_lanes_by_toll_id as get_from_memory_cache;
    let cached_lanes = get_from_memory_cache(toll_id);

    if !cached_lanes.is_empty() {
        tracing::debug!(
            toll_id,
            lanes = cached_lanes.len(),
            "[Cache] TollLane cache hit by toll_id"
        );
        // Convert từ TOLL_LANE sang TollLaneDto để return
        return cached_lanes
            .into_iter()
            .map(|lane| TollLaneDto {
                toll_lane_id: lane.toll_lane_id,
                toll_id: lane.toll_id,
                lane_code: lane.lane_code,
                lane_type: lane.lane_type,
                lane_name: lane.lane_name,
                status: lane.status,
                free_lanes: lane.free_lanes,
                free_lanes_limit: lane.free_lanes_limit,
                free_lanes_second: lane.free_lanes_second,
                free_toll_id: lane.free_toll_id,
                dup_filter: lane.dup_filter,
                free_limit_time: lane.free_limit_time,
                free_allow_loop: lane.free_allow_loop,
                transit_lane: lane.transit_lane,
                freeflow_lane: lane.freeflow_lane,
            })
            .collect();
    }

    tracing::debug!(toll_id, "[Cache] TollLane cache miss by toll_id");
    match get_toll_lanes_by_toll_id_from_db(db_pool.clone(), toll_id).await {
        Ok(lanes) => {
            let mut cached_lanes = Vec::new();
            for lane in lanes {
                let key = cache.gen_key(CachePrefix::TollLane, &[&lane.toll_id, &lane.lane_code]);
                // Lưu vào cache
                cache.set(&key, &lane).await;
                // Cũng load vào in-memory cache
                let toll_lane = lane.to_toll_lane();
                save_toll_lane(toll_lane);
                cached_lanes.push(lane);
            }
            tracing::debug!(
                toll_id,
                lanes = cached_lanes.len(),
                "[Cache] TollLane loaded by toll_id"
            );
            cached_lanes
        }
        Err(e) => {
            tracing::error!(toll_id, error = ?e, "[Cache] TollLane DB query by toll_id failed");
            Vec::new()
        }
    }
}

/// Query toll_lane từ DB theo toll_id và lane_code
#[allow(dead_code)]
async fn get_toll_lane_from_db(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    toll_id: i32,
    lane_code: i32,
) -> Result<Option<TollLaneDto>, DbError> {
    use crate::db::format_sql_value;

    task::spawn_blocking(move || {
        let query = format!(
            r#"SELECT TOLL_LANE_ID, TOLL_ID, LANE_CODE, LANE_TYPE, LANE_NAME, STATUS, 
                      FREE_LANES, FREE_LANES_LIMIT, FREE_LANES_SECOND, FREE_TOLL_ID, 
                      DUP_FILTER, FREE_LIMIT_TIME, FREE_ALLOW_LOOP, TRANSIT_LANE, FREEFLOW_LANE
               FROM RATING_OWNER.TOLL_LANE
               WHERE STATUS = '1' AND TOLL_ID = {} AND LANE_CODE = {}"#,
            format_sql_value(&toll_id),
            format_sql_value(&lane_code)
        );

        let conn = get_connection_with_retry(db_pool.as_ref())?;
        let cursor_result = conn.execute(&query, (), None)?;

        match cursor_result {
            Some(mut cursor) => match cursor.next_row() {
                Ok(Some(mut row)) => {
                    let toll_lane_id = get_i32(&mut row, 1)?;
                    let toll_id = get_i32(&mut row, 2)?;
                    let lane_code = get_i32(&mut row, 3)?;
                    let lane_type = get_nullable_string(&mut row, 4)?.unwrap_or_default();
                    let lane_name = get_nullable_string(&mut row, 5)?.unwrap_or_default();
                    let status = get_nullable_string(&mut row, 6)?.unwrap_or_default();
                    let free_lanes = get_nullable_string(&mut row, 7)?.unwrap_or_default();
                    let free_lanes_limit = get_nullable_i32(&mut row, 8)?;
                    let free_lanes_second = get_nullable_string(&mut row, 9)?.unwrap_or_default();
                    let free_toll_id = get_nullable_i32(&mut row, 10)?;
                    let dup_filter = get_nullable_string(&mut row, 11)?.unwrap_or_default();
                    let free_limit_time = get_nullable_string(&mut row, 12)?.unwrap_or_default();
                    let free_allow_loop = get_nullable_string(&mut row, 13)?.unwrap_or_default();
                    let transit_lane = get_nullable_string(&mut row, 14)?.unwrap_or_default();
                    let freeflow_lane = get_nullable_string(&mut row, 15)?.unwrap_or_default();

                    Ok(Some(TollLaneDto {
                        toll_lane_id,
                        toll_id,
                        lane_code,
                        lane_type,
                        lane_name,
                        status,
                        free_lanes,
                        free_lanes_limit,
                        free_lanes_second,
                        free_toll_id,
                        dup_filter,
                        free_limit_time,
                        free_allow_loop,
                        transit_lane,
                        freeflow_lane,
                    }))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(DbError::ExecutionError(format!(
                    "[TollLaneCache] Error fetching row: {}",
                    e
                ))),
            },
            None => Ok(None),
        }
    })
    .await
    .map_err(|e| {
        DbError::ExecutionError(format!("[TollLaneCache] spawn_blocking join error: {}", e))
    })?
}

/// Query tất cả toll_lanes từ DB theo toll_id
async fn get_toll_lanes_by_toll_id_from_db(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    toll_id: i32,
) -> Result<Vec<TollLaneDto>, DbError> {
    use crate::db::format_sql_value;

    task::spawn_blocking(move || {
        let query = format!(
            r#"SELECT TOLL_LANE_ID, TOLL_ID, LANE_CODE, LANE_TYPE, LANE_NAME, STATUS, 
                      FREE_LANES, FREE_LANES_LIMIT, FREE_LANES_SECOND, FREE_TOLL_ID, 
                      DUP_FILTER, FREE_LIMIT_TIME, FREE_ALLOW_LOOP, TRANSIT_LANE, FREEFLOW_LANE
               FROM RATING_OWNER.TOLL_LANE
               WHERE STATUS = '1' AND TOLL_ID = {}"#,
            format_sql_value(&toll_id)
        );

        let mut result = Vec::new();
        let conn = get_connection_with_retry(db_pool.as_ref())?;
        let cursor_result = conn.execute(&query, (), None)?;

        if let Some(mut cursor) = cursor_result {
            loop {
                match cursor.next_row() {
                    Ok(Some(mut row)) => {
                        let toll_lane_id = get_i32(&mut row, 1)?;
                        let toll_id = get_i32(&mut row, 2)?;
                        let lane_code = get_i32(&mut row, 3)?;
                        let lane_type = get_nullable_string(&mut row, 4)?.unwrap_or_default();
                        let lane_name = get_nullable_string(&mut row, 5)?.unwrap_or_default();
                        let status = get_nullable_string(&mut row, 6)?.unwrap_or_default();
                        let free_lanes = get_nullable_string(&mut row, 7)?.unwrap_or_default();
                        let free_lanes_limit = get_nullable_i32(&mut row, 8)?;
                        let free_lanes_second =
                            get_nullable_string(&mut row, 9)?.unwrap_or_default();
                        let free_toll_id = get_nullable_i32(&mut row, 10)?;
                        let dup_filter = get_nullable_string(&mut row, 11)?.unwrap_or_default();
                        let free_limit_time =
                            get_nullable_string(&mut row, 12)?.unwrap_or_default();
                        let free_allow_loop =
                            get_nullable_string(&mut row, 13)?.unwrap_or_default();
                        let transit_lane = get_nullable_string(&mut row, 14)?.unwrap_or_default();
                        let freeflow_lane = get_nullable_string(&mut row, 15)?.unwrap_or_default();

                        let dto = TollLaneDto {
                            toll_lane_id,
                            toll_id,
                            lane_code,
                            lane_type,
                            lane_name,
                            status,
                            free_lanes,
                            free_lanes_limit,
                            free_lanes_second,
                            free_toll_id,
                            dup_filter,
                            free_limit_time,
                            free_allow_loop,
                            transit_lane,
                            freeflow_lane,
                        };
                        result.push(dto);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return Err(DbError::ExecutionError(format!(
                            "[TollLaneCache] Error fetching row: {}",
                            e
                        )))
                    }
                }
            }
        }

        Ok(result)
    })
    .await
    .map_err(|e| {
        DbError::ExecutionError(format!("[TollLaneCache] spawn_blocking join error: {}", e))
    })?
}

async fn get_active_toll_lanes(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
) -> Result<Vec<TollLaneDto>, DbError> {
    task::spawn_blocking(move || {
        let query = r#"SELECT TOLL_LANE_ID, TOLL_ID, LANE_CODE, LANE_TYPE, LANE_NAME, STATUS, 
                  FREE_LANES, FREE_LANES_LIMIT, FREE_LANES_SECOND, FREE_TOLL_ID, 
                  DUP_FILTER, FREE_LIMIT_TIME, FREE_ALLOW_LOOP, TRANSIT_LANE, FREEFLOW_LANE
            FROM RATING_OWNER.TOLL_LANE
            WHERE STATUS = '1'"#;
        let mut result = Vec::new();
        let conn = get_connection_with_retry(db_pool.as_ref())?;
        let cursor_result = conn.execute(query, (), None)?;
        if let Some(mut cursor) = cursor_result {
            loop {
                match cursor.next_row() {
                    Ok(Some(mut row)) => {
                        let toll_lane_id = get_i32(&mut row, 1)?;
                        let toll_id = get_i32(&mut row, 2)?;
                        let lane_code = get_i32(&mut row, 3)?;
                        let lane_type = get_nullable_string(&mut row, 4)?.unwrap_or_default();
                        let lane_name = get_nullable_string(&mut row, 5)?.unwrap_or_default();
                        let status = get_nullable_string(&mut row, 6)?.unwrap_or_default();
                        let free_lanes = get_nullable_string(&mut row, 7)?.unwrap_or_default();
                        let free_lanes_limit = get_nullable_i32(&mut row, 8)?;
                        let free_lanes_second =
                            get_nullable_string(&mut row, 9)?.unwrap_or_default();
                        let free_toll_id = get_nullable_i32(&mut row, 10)?;
                        let dup_filter = get_nullable_string(&mut row, 11)?.unwrap_or_default();
                        let free_limit_time =
                            get_nullable_string(&mut row, 12)?.unwrap_or_default();
                        let free_allow_loop =
                            get_nullable_string(&mut row, 13)?.unwrap_or_default();
                        let transit_lane = get_nullable_string(&mut row, 14)?.unwrap_or_default();
                        let freeflow_lane = get_nullable_string(&mut row, 15)?.unwrap_or_default();

                        let dto = TollLaneDto {
                            toll_lane_id,
                            toll_id,
                            lane_code,
                            lane_type,
                            lane_name,
                            status,
                            free_lanes,
                            free_lanes_limit,
                            free_lanes_second,
                            free_toll_id,
                            dup_filter,
                            free_limit_time,
                            free_allow_loop,
                            transit_lane,
                            freeflow_lane,
                        };
                        result.push(dto);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return Err(DbError::ExecutionError(format!(
                            "Error fetching row: {}",
                            e
                        )))
                    }
                }
            }
        }

        Ok(result)
    })
    .await
    .map_err(|e| DbError::ExecutionError(format!("spawn_blocking join error: {}", e)))?
}
