//! Cache bảng giá (price list) theo etag, get_price_cache, load from DB.
//! In-memory layer uses DashMap for concurrent get/set when cache is large.

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use odbc_api::Cursor;
use once_cell::sync::Lazy;
use r2d2::Pool;
use tokio::task;

use crate::{
    cache::{
        config::{cache_manager::CacheManager, cache_prefix::CachePrefix},
        data::dto::price_dto::PriceDto,
    },
    configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager},
    db::{get_i64, get_nullable_datetime_string, get_nullable_i64, get_nullable_string, DbError},
};

/// In-memory layer: key (same as CacheManager) -> PriceDto. DashMap for concurrent access.
static PRICE_MEMORY: Lazy<DashMap<String, PriceDto>> = Lazy::new(DashMap::new);
/// Index by (stage_id, vehicle_type) for get_price_by_stage_id. DashMap for concurrent access.
static STAGE_PRICE_MEMORY: Lazy<DashMap<(i64, String), PriceDto>> = Lazy::new(DashMap::new);

fn replace_all_prices(data: Vec<(String, PriceDto)>) {
    let map: HashMap<_, _> = data.iter().cloned().collect();
    let mut stage_map = HashMap::new();
    for (_, dto) in &map {
        let sid = dto.stage_id.unwrap_or(0);
        let vt = dto.vehicle_type.as_deref().unwrap_or("None").to_string();
        stage_map.insert((sid, vt), dto.clone());
    }
    PRICE_MEMORY.clear();
    for (k, v) in map {
        PRICE_MEMORY.insert(k, v);
    }
    STAGE_PRICE_MEMORY.clear();
    for (k, v) in stage_map {
        STAGE_PRICE_MEMORY.insert(k, v);
    }
}

fn get_price_from_memory(key: &str) -> Option<PriceDto> {
    PRICE_MEMORY.get(key).map(|r| r.clone())
}

fn set_price_memory(key: &str, value: &PriceDto) {
    PRICE_MEMORY.insert(key.to_string(), value.clone());
}

/// Load cache Price. Prefer DB; khi startup nếu DB failed hoặc pool None thì fallback KeyDB (khi `load_from_keydb` true). Reload luôn từ DB.
/// `pool_opt`: Option vì RATING có thể chưa sẵn sàng lúc khởi động; khi None to omit DB.
pub async fn get_price_cache(
    pool_opt: Option<Arc<Pool<OdbcConnectionManager>>>,
    cache: Arc<CacheManager>,
    load_from_keydb: bool,
) {
    let result = match pool_opt {
        Some(pool) => get_active_price(pool).await,
        None => {
            tracing::warn!(
                "[Cache] Price skip DB (pool not available), will use KeyDB fallback if enabled"
            );
            Err(crate::db::error::DbError::ExecutionError(
                "DB pool not available".to_string(),
            ))
        }
    };
    match result {
        Ok(mut prices) => {
            // Một key cache = (toll_a, toll_b, vehicle_type). Nếu DB trả nhiều row trùng key, giữ bản có effect_datetime mới nhất.
            prices.sort_by(|a, b| {
                let ka = (
                    a.toll_a.unwrap_or(0),
                    a.toll_b.unwrap_or(0),
                    a.vehicle_type.as_deref().unwrap_or(""),
                );
                let kb = (
                    b.toll_a.unwrap_or(0),
                    b.toll_b.unwrap_or(0),
                    b.vehicle_type.as_deref().unwrap_or(""),
                );
                ka.cmp(&kb).then_with(|| {
                    let da = a.effect_datetime.as_deref().unwrap_or("");
                    let db = b.effect_datetime.as_deref().unwrap_or("");
                    da.cmp(db) // ascending: cũ trước, mới sau → khi collect HashMap bản cuối (mới nhất) thắng
                })
            });
            let cache_data: Vec<(String, PriceDto)> = prices
                .into_iter()
                .map(|price| {
                    let toll_a_str = price
                        .toll_a
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "None".to_string());
                    let toll_b_str = price
                        .toll_b
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "None".to_string());
                    let vehicle_type_str =
                        price.vehicle_type.as_deref().unwrap_or("None").to_string();
                    let price_key = cache.gen_key(
                        CachePrefix::Price,
                        &[
                            &toll_a_str as &dyn std::fmt::Display,
                            &toll_b_str as &dyn std::fmt::Display,
                            &vehicle_type_str as &dyn std::fmt::Display,
                        ],
                    );
                    (price_key, price)
                })
                .collect();
            let n = cache_data.len();
            cache
                .atomic_reload_prefix(CachePrefix::Price.as_str(), cache_data.clone())
                .await;
            replace_all_prices(cache_data);
            if load_from_keydb {
                tracing::info!(count = n, "[Cache] Price loaded from DB, synced to KeyDB");
            } else {
                tracing::info!(count = n, "[Cache] Price loaded from DB");
            }
        }
        Err(e) => {
            tracing::error!(error = ?e, "[Cache] Price load from DB failed");
            if !load_from_keydb {
                return;
            }
            let prefix = CachePrefix::Price.as_str();
            let keydb_loaded = cache.load_prefix_from_keydb::<PriceDto>(prefix).await;
            let n = keydb_loaded.len();
            if n == 0 {
                tracing::warn!("[Cache] Price fallback KeyDB empty, keeping empty");
                return;
            }
            cache
                .atomic_reload_prefix(prefix, keydb_loaded.clone())
                .await;
            replace_all_prices(keydb_loaded);
            tracing::info!(
                count = n,
                "[Cache] Price loaded from KeyDB (DB failed, fallback)"
            );
        }
    }
}

/// Lấy price từ cache, nếu không có thì query DB và cache lại (cache-aside pattern).
/// Luồng tính phí: chỉ memory rồi fallback DB, không đọc KeyDB.
pub async fn get_price(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
    toll_a: i64,
    toll_b: i64,
    vehicle_type: &str,
) -> Option<PriceDto> {
    let vehicle_type_str = if vehicle_type.is_empty() {
        "None".to_string()
    } else {
        vehicle_type.to_string()
    };
    let price_key = cache.gen_key(
        CachePrefix::Price,
        &[
            &toll_a as &dyn std::fmt::Display,
            &toll_b as &dyn std::fmt::Display,
            &vehicle_type_str as &dyn std::fmt::Display,
        ],
    );
    let price_key_reverse = cache.gen_key(
        CachePrefix::Price,
        &[
            &toll_b as &dyn std::fmt::Display,
            &toll_a as &dyn std::fmt::Display,
            &vehicle_type_str as &dyn std::fmt::Display,
        ],
    );

    if let Some(price) = get_price_from_memory(&price_key) {
        tracing::debug!(toll_a, toll_b, "[Cache] Price in-memory hit");
        return Some(price);
    }
    if let Some(price) = get_price_from_memory(&price_key_reverse) {
        tracing::debug!(toll_a, toll_b, "[Cache] Price in-memory hit (reverse)");
        return Some(swap_toll_direction(&price));
    }

    tracing::debug!(
        toll_a,
        toll_b,
        "[Cache] Price in-memory miss, fallback to DB"
    );
    match get_price_from_db(db_pool.clone(), toll_a, toll_b, vehicle_type).await {
        Ok(Some(price)) => {
            cache.set(&price_key, &price).await;
            set_price_memory(&price_key, &price);
            tracing::debug!(toll_a, toll_b, "[Cache] Price loaded from DB");
            return Some(price);
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(toll_a, toll_b, error = ?e, "[Cache] Price DB query failed");
            return None;
        }
    }

    // Thử chiều ngược (b -> a) trong DB
    match get_price_from_db(db_pool, toll_b, toll_a, vehicle_type).await {
        Ok(Some(price)) => {
            let price_swapped = swap_toll_direction(&price);
            cache.set(&price_key, &price_swapped).await;
            set_price_memory(&price_key, &price_swapped);
            tracing::debug!(toll_a, toll_b, "[Cache] Price loaded from DB (reverse)");
            Some(price_swapped)
        }
        Ok(None) => {
            tracing::warn!(
                toll_a,
                toll_b,
                "[Cache] Price not found in DB (both directions)"
            );
            None
        }
        Err(e) => {
            tracing::error!(toll_a, toll_b, error = ?e, "[Cache] Price DB query (reverse) failed");
            None
        }
    }
}

/// Đổi chiều toll_a/toll_b trong PriceDto (dùng khi lấy giá chiều b->a để trả về theo chiều a->b).
fn swap_toll_direction(price: &PriceDto) -> PriceDto {
    PriceDto {
        toll_a: price.toll_b,
        toll_b: price.toll_a,
        ..price.clone()
    }
}

/// Get price by STAGE_ID and vehicle_type. Used for segments from CLOSED_CYCLE_TRANSITION_STAGE: after WB/Ex/Blacklist check, get price by stage_id.
/// Memory (stage index) -> DB; result written back to STAGE_PRICE_MEMORY.
pub async fn get_price_by_stage_id(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
    stage_id: i64,
    vehicle_type: &str,
) -> Option<PriceDto> {
    let _ = cache;
    let vehicle_type_str = if vehicle_type.is_empty() {
        "None".to_string()
    } else {
        vehicle_type.to_string()
    };
    if let Some(p) = STAGE_PRICE_MEMORY.get(&(stage_id, vehicle_type_str.clone())) {
        tracing::debug!(stage_id, "[Cache] Price by stage_id in-memory hit");
        return Some(p.clone());
    }
    match get_price_by_stage_id_from_db(db_pool.clone(), stage_id, vehicle_type).await {
        Ok(Some(price)) => {
            STAGE_PRICE_MEMORY.insert((stage_id, vehicle_type_str), price.clone());
            tracing::debug!(stage_id, "[Cache] Price by stage_id loaded from DB");
            Some(price)
        }
        Ok(None) => None,
        Err(e) => {
            tracing::error!(stage_id, error = ?e, "[Cache] Price by stage_id DB query failed");
            None
        }
    }
}

async fn get_price_by_stage_id_from_db(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    stage_id: i64,
    vehicle_type: &str,
) -> Result<Option<PriceDto>, DbError> {
    use crate::db::{format_sql_string, format_sql_value};

    let vehicle_type = vehicle_type.to_string();
    let now_utc = crate::utils::now_utc_db_string();
    let now_utc_sql = format_sql_string(&now_utc);

    task::spawn_blocking(move || {
        let escaped_vehicle_type = vehicle_type.replace("'", "''");
        let query = format!(
            r#"SELECT P.PRICE_ID, T.TOLL_A, T.TOLL_B, T.BOO, P.PRICE_AMOUNT, P.VEHICLE_TYPE,
                      P.PRICE_TICKET_TYPE, P.PRICE_TYPE, P.TURNING_CODE, TO_CHAR(P.EFFECT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), TO_CHAR(P.END_DATETIME, 'YYYY-MM-DD HH24:MI:SS'),
                      BT.BOT_ID, P.STAGE_ID
               FROM RATING_OWNER.PRICE P
               INNER JOIN RATING_OWNER.TOLL_STAGE T ON P.STAGE_ID = T.STAGE_ID
               LEFT JOIN RATING_OWNER.BOT_TOLL BT ON BT.STAGE_ID = P.STAGE_ID
               WHERE P.STATUS = '1'
                AND P.STAGE_ID = {} AND P.VEHICLE_TYPE = {}
                AND P.EFFECT_DATETIME <= TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS')
                AND (P.END_DATETIME IS NULL OR P.END_DATETIME >= TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS'))"#,
            format_sql_value(&stage_id),
            format_sql_string(&escaped_vehicle_type),
            now_utc_sql,
            now_utc_sql
        );
        let conn = get_connection_with_retry(db_pool.as_ref())?;
        let cursor_result = conn.execute(&query, (), None)?;
        match cursor_result {
            Some(mut cursor) => match cursor.next_row() {
                Ok(Some(mut row)) => {
                    let price_id = get_i64(&mut row, 1)?;
                    let toll_a = get_nullable_i64(&mut row, 2)?;
                    let toll_b = get_nullable_i64(&mut row, 3)?;
                    let boo = get_nullable_string(&mut row, 4)?;
                    let price_amount = get_nullable_i64(&mut row, 5)?;
                    let vehicle_type = get_nullable_string(&mut row, 6)?;
                    let price_ticket_type = get_nullable_string(&mut row, 7)?;
                    let price_type = get_nullable_string(&mut row, 8)?;
                    let turning_code = get_nullable_string(&mut row, 9)?;
                    let effect_datetime = get_nullable_datetime_string(&mut row, 10)?;
                    let end_datetime = get_nullable_datetime_string(&mut row, 11)?;
                    let bot_id = get_nullable_i64(&mut row, 12)?;
                    let stage_id = get_nullable_i64(&mut row, 13)?;
                    Ok(Some(PriceDto {
                        price_id,
                        toll_a,
                        toll_b,
                        boo,
                        price_amount,
                        vehicle_type,
                        price_ticket_type,
                        price_type,
                        turning_code,
                        effect_datetime,
                        end_datetime,
                        bot_id,
                        stage_id,
                    }))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(DbError::ExecutionError(format!(
                    "[PriceCache] Error fetching row by stage_id: {}",
                    e
                ))),
            },
            None => Ok(None),
        }
    })
    .await
    .map_err(|e| DbError::ExecutionError(format!("[PriceCache] get_price_by_stage_id spawn_blocking: {}", e)))?
}

/// Query price từ DB theo toll_a, toll_b, vehicle_type (không lọc theo PRICE_TYPE).
/// Thời gian so sánh hiệu lực luôn UTC (truyền từ ứng dụng).
async fn get_price_from_db(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    toll_a: i64,
    toll_b: i64,
    vehicle_type: &str,
) -> Result<Option<PriceDto>, DbError> {
    use crate::db::{format_sql_string, format_sql_value};

    let vehicle_type = vehicle_type.to_string();
    let now_utc = crate::utils::now_utc_db_string();
    let now_utc_sql = format_sql_string(&now_utc);

    task::spawn_blocking(move || {
        let escaped_vehicle_type = vehicle_type.replace("'", "''");

        let query = format!(
            r#"SELECT P.PRICE_ID, T.TOLL_A, T.TOLL_B, T.BOO, P.PRICE_AMOUNT, P.VEHICLE_TYPE,
                      P.PRICE_TICKET_TYPE, P.PRICE_TYPE, P.TURNING_CODE, TO_CHAR(P.EFFECT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), TO_CHAR(P.END_DATETIME, 'YYYY-MM-DD HH24:MI:SS'),
                      BT.BOT_ID, P.STAGE_ID
               FROM RATING_OWNER.PRICE P
               INNER JOIN RATING_OWNER.TOLL_STAGE T ON P.STAGE_ID = T.STAGE_ID
               LEFT JOIN RATING_OWNER.BOT_TOLL BT ON BT.STAGE_ID = P.STAGE_ID
               WHERE P.STATUS = '1'
                AND T.TOLL_A = {} AND T.TOLL_B = {} AND P.VEHICLE_TYPE = {}
                AND P.EFFECT_DATETIME <= TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS') + 1/24
                AND (P.END_DATETIME IS NULL OR P.END_DATETIME >= TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS') - 1/24)"#,
            format_sql_value(&toll_a),
            format_sql_value(&toll_b),
            format_sql_string(&escaped_vehicle_type),
            now_utc_sql,
            now_utc_sql
        );

        let conn = get_connection_with_retry(db_pool.as_ref())?;
        let cursor_result = conn.execute(&query, (), None)?;

        match cursor_result {
            Some(mut cursor) => {
                match cursor.next_row() {
                    Ok(Some(mut row)) => {
                        let price_id = get_i64(&mut row, 1)?;
                        let toll_a = get_nullable_i64(&mut row, 2)?;
                        let toll_b = get_nullable_i64(&mut row, 3)?;
                        let boo = get_nullable_string(&mut row, 4)?;
                        let price_amount = get_nullable_i64(&mut row, 5)?;
                        let vehicle_type = get_nullable_string(&mut row, 6)?;
                        let price_ticket_type = get_nullable_string(&mut row, 7)?;
                        let price_type = get_nullable_string(&mut row, 8)?;
                        let turning_code = get_nullable_string(&mut row, 9)?;
                        let effect_datetime = get_nullable_datetime_string(&mut row, 10)?;
                        let end_datetime = get_nullable_datetime_string(&mut row, 11)?;
                        let bot_id = get_nullable_i64(&mut row, 12)?;
                        let stage_id = get_nullable_i64(&mut row, 13)?;

                        Ok(Some(PriceDto {
                            price_id,
                            toll_a,
                            toll_b,
                            boo,
                            price_amount,
                            vehicle_type,
                            price_ticket_type,
                            price_type,
                            turning_code,
                            effect_datetime,
                            end_datetime,
                            bot_id,
                            stage_id,
                        }))
                    }
                    Ok(None) => Ok(None),
                    Err(e) => Err(DbError::ExecutionError(
                        format!("[PriceCache] Error fetching row: {}", e),
                    )),
                }
            }
            None => Ok(None),
        }
    })
    .await
    .map_err(|e| DbError::ExecutionError(format!("[PriceCache] spawn_blocking join error: {}", e)))?
}

async fn get_active_price(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
) -> Result<Vec<PriceDto>, DbError> {
    let now_utc = crate::utils::now_utc_db_string();
    let now_utc_sql = crate::db::format_sql_string(&now_utc);

    task::spawn_blocking(move || {
    let query = format!(
        r#"SELECT P.PRICE_ID, T.TOLL_A, T.TOLL_B, T.BOO, P.PRICE_AMOUNT, P.VEHICLE_TYPE, P.PRICE_TICKET_TYPE, P.PRICE_TYPE, P.TURNING_CODE, TO_CHAR(P.EFFECT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), TO_CHAR(P.END_DATETIME, 'YYYY-MM-DD HH24:MI:SS'),
           BT.BOT_ID, P.STAGE_ID
           FROM RATING_OWNER.PRICE P
           INNER JOIN RATING_OWNER.TOLL_STAGE T ON P.STAGE_ID = T.STAGE_ID
           LEFT JOIN RATING_OWNER.BOT_TOLL BT ON BT.STAGE_ID = P.STAGE_ID
           WHERE P.STATUS = '1'
           AND P.EFFECT_DATETIME <= TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS') + 1/24
           AND (P.END_DATETIME IS NULL OR P.END_DATETIME >= TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS') - 1/24)"#,
        now_utc_sql,
        now_utc_sql
    );
    let mut result = Vec::new();
    let conn = get_connection_with_retry(db_pool.as_ref())?;
    let cursor_result = conn.execute(&query, (), None)?;
    if let Some(mut cursor) = cursor_result {
        loop {
            match cursor.next_row() {
                    Ok(Some(mut row)) => {
                        let price_id = get_i64(&mut row, 1)?;
                        let toll_a = get_nullable_i64(&mut row, 2)?;
                        let toll_b = get_nullable_i64(&mut row, 3)?;
                        let boo = get_nullable_string(&mut row, 4)?;
                        let price_amount = get_nullable_i64(&mut row, 5)?;
                        let vehicle_type = get_nullable_string(&mut row, 6)?;
                        let price_ticket_type = get_nullable_string(&mut row, 7)?;
                        let price_type = get_nullable_string(&mut row, 8)?;
                        let turning_code = get_nullable_string(&mut row, 9)?;
                        let effect_datetime = get_nullable_datetime_string(&mut row, 10)?;
                        let end_datetime = get_nullable_datetime_string(&mut row, 11)?;
                        let bot_id = get_nullable_i64(&mut row, 12)?;
                        let stage_id = get_nullable_i64(&mut row, 13)?;
                        let dto = PriceDto {
                            price_id,
                            toll_a,
                            toll_b,
                            boo,
                            price_amount,
                            vehicle_type,
                            price_ticket_type,
                            price_type,
                            turning_code,
                            effect_datetime,
                            end_datetime,
                            bot_id,
                            stage_id,
                        };
                        result.push(dto);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return Err(DbError::ExecutionError(
                            format!("[PriceCache] Error fetching row: {}", e),
                        ))
                    }
                }
            }
        }

    Ok(result)
    })
    .await
    .map_err(|e| DbError::ExecutionError(format!("[PriceCache] spawn_blocking join error: {}", e)))?
}
