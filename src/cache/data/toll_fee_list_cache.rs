//! Cache bảng phí (toll_fee_list) theo etag, get_toll_fee_list_cache, load from DB.

use std::collections::HashMap;
use std::sync::Arc;

use odbc_api::Cursor;
use r2d2::Pool;
use tokio::task;

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::config::cache_prefix::CachePrefix;
use crate::cache::data::dto::ex_list_dto::ExListItemDto;
use crate::cache::data::dto::ex_price_list_dto::ExPriceListItemDto;
use crate::cache::data::dto::wb_list_route_dto::WbListRouteDto;
use crate::cache::data::ex_list_cache;
use crate::cache::data::ex_price_list_cache;
use crate::cache::data::wb_route_cache;
use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::{
    get_i64, get_nullable_datetime_string, get_nullable_i64, get_nullable_string, DbError,
};
use crate::services::toll_fee::TollFeeListContext;
use crate::utils::normalize_etag;

/// Trả về true nếu lỗi ODBC là "table or view was not found" (vd. WB_LIST/EX_LIST/EX_PRICE_LIST chưa deploy).
fn is_table_or_view_not_found(e: &DbError) -> bool {
    match e {
        DbError::ExecutionError(msg) => {
            msg.contains("Table or view was not found")
                || msg.contains("42S02")
                || msg.contains("200753")
        }
        _ => false,
    }
}

/// Key cache: prefix:etag (ví dụ "8_wb_route:3416214B9400F274TTTT0001").
fn wb_route_key(etag: &str) -> String {
    format!("{}:{}", CachePrefix::WbRoute.as_str(), etag)
}
fn ex_list_key(etag: &str) -> String {
    format!("{}:{}", CachePrefix::ExList.as_str(), etag)
}
fn ex_price_list_key(etag: &str) -> String {
    format!("{}:{}", CachePrefix::ExPriceList.as_str(), etag)
}

/// Lấy TollFeeListContext theo etag: ưu tiên cache, cache miss thì query DB theo etag và ghi lại cache (cache-aside).
/// Ba nhánh WB / Ex list / Ex price list được lấy cache song song; nếu miss thì load DB song song để giảm latency.
/// Trả về None nếu etag rỗng; ngược lại trả về context (có thể danh sách rỗng nếu etag không có trong DB).
/// Etag được chuẩn hóa (trim + bỏ null cuối) để khớp với key cache khi reload (group_by_etag_* dùng normalize_etag).
pub async fn get_toll_fee_list_context(
    _db_pool: Arc<Pool<OdbcConnectionManager>>,
    _cache: Arc<CacheManager>,
    etag: &str,
) -> Option<TollFeeListContext> {
    let etag = normalize_etag(etag);
    if etag.is_empty() {
        return None;
    }

    // Lấy cache song song cho cả ba nhánh
    let (wb_opt, ex_opt, ex_price_opt) = tokio::join!(
        wb_route_cache::get_wb_route_by_etag(&etag),
        ex_list_cache::get_ex_list_by_etag(&etag),
        ex_price_list_cache::get_ex_price_list_by_etag(&etag),
    );

    // Cache miss thì load DB song song cho từng nhánh thiếu
    let (wb_list, ex_list_vec, ex_price_list_vec) = tokio::join!(
        async {
            match wb_opt {
                Some(list) => list,
                None => {
                    tracing::debug!(etag = %etag, "[Cache] TollFeeList WB cache miss");
                    Vec::new()
                }
            }
        },
        async {
            match ex_opt {
                Some(list) => list,
                None => {
                    tracing::debug!(etag = %etag, "[Cache] TollFeeList Ex list cache miss");
                    Vec::new()
                }
            }
        },
        async {
            match ex_price_opt {
                Some(list) => list,
                None => {
                    tracing::debug!(etag = %etag, "[Cache] TollFeeList Ex price list cache miss");
                    Vec::new()
                }
            }
        },
    );

    let mut white_list = Vec::with_capacity(wb_list.len());
    let mut black_list = Vec::with_capacity(wb_list.len());
    for w in &wb_list {
        let item = (w.toll_id, w.cycle_id, w.boo.clone());
        if w.item_type.eq_ignore_ascii_case("W") {
            white_list.push(item);
        } else if w.item_type.eq_ignore_ascii_case("B") {
            black_list.push(item);
        }
    }
    let ex_list: Vec<(i64, Option<String>, String, Option<String>)> = ex_list_vec
        .iter()
        .map(|e| {
            (
                e.station_id,
                e.boo.clone(),
                e.vehicle_type.clone(),
                e.type_station.clone(),
            )
        })
        .collect();
    let ex_price_list: Vec<(i64, Option<String>, i32, Option<String>)> = ex_price_list_vec
        .iter()
        .map(|e| {
            let pt = crate::price_ticket_type::from_str(&e.price_ticket_type);
            (e.station_id, e.boo.clone(), pt, e.type_station.clone())
        })
        .collect();

    let all_empty = white_list.is_empty()
        && black_list.is_empty()
        && ex_list.is_empty()
        && ex_price_list.is_empty();
    if all_empty {
        tracing::debug!(etag = %etag, "[Cache] TollFeeList context: WB/Ex/ExPrice lists all empty for etag");
    } else if white_list.is_empty() && !wb_list.is_empty() {
        let n_w = wb_list
            .iter()
            .filter(|w| w.item_type.eq_ignore_ascii_case("W"))
            .count();
        tracing::debug!(etag = %etag, wb_list_len = wb_list.len(), n_whitelist_type = n_w, "[Cache] TollFeeList context: WB loaded but white_list empty (no W entries?)");
    } else if !white_list.is_empty() {
        tracing::debug!(etag = %etag, white_list_len = white_list.len(), "[Cache] TollFeeList context: white_list non-empty");
    }

    Some(TollFeeListContext {
        white_list,
        black_list,
        ex_list,
        ex_price_list,
    })
}

/// Load cache TollFeeList (WB, Ex, ExPrice). Prefer DB; khi startup nếu DB failed hoặc pool None thì fallback KeyDB (khi `load_from_keydb` true). Reload luôn từ DB.
/// `pool_opt`: Option vì RATING có thể chưa sẵn sàng lúc khởi động; khi None to omit DB.
pub async fn get_toll_fee_list_cache(
    pool_opt: Option<Arc<Pool<OdbcConnectionManager>>>,
    cache: Arc<CacheManager>,
    load_from_keydb: bool,
) {
    wb_route_cache::set_cache(cache.clone());
    ex_list_cache::set_cache(cache.clone());
    ex_price_list_cache::set_cache(cache.clone());
    let (wb_res, ex_res, ex_price_res) = match pool_opt {
        Some(pool) => tokio::join!(
            load_wb_list_from_db(pool.clone()),
            load_ex_list_from_db(pool.clone()),
            load_ex_price_list_from_db(pool),
        ),
        None => {
            tracing::warn!("[Cache] TollFeeList skip DB (pool not available), will use KeyDB fallback if enabled");
            let e = DbError::ExecutionError("DB pool not available".to_string());
            (
                Err::<Vec<WbListRouteDto>, _>(e),
                Err::<Vec<ExListItemDto>, _>(DbError::ExecutionError(
                    "DB pool not available".to_string(),
                )),
                Err::<Vec<ExPriceListItemDto>, _>(DbError::ExecutionError(
                    "DB pool not available".to_string(),
                )),
            )
        }
    };

    let wb_prefix = CachePrefix::WbRoute.as_str();
    let ex_prefix = CachePrefix::ExList.as_str();
    let ex_price_prefix = CachePrefix::ExPriceList.as_str();

    if let Ok(rows) = wb_res {
        let wb_by_etag: HashMap<String, Vec<WbListRouteDto>> = group_by_etag_wb(rows);
        let n = wb_by_etag.len();
        let wb_cache_data: Vec<(String, Vec<WbListRouteDto>)> = wb_by_etag
            .iter()
            .map(|(etag, list)| (wb_route_key(etag), list.clone()))
            .collect();
        cache.atomic_reload_prefix(wb_prefix, wb_cache_data).await;
        wb_route_cache::fill_memory_from_reload(wb_by_etag);
        if load_from_keydb {
            tracing::info!(
                count = n,
                "[Cache] TollFeeList WB list loaded from DB, synced to KeyDB"
            );
        } else {
            tracing::info!(count = n, "[Cache] TollFeeList WB list loaded from DB");
        }
    } else {
        if let Err(e) = &wb_res {
            if !is_table_or_view_not_found(e) {
                tracing::warn!(error = ?e, "[Cache] TollFeeList WB list load from DB failed");
            }
        }
        if load_from_keydb {
            let wb_keydb = cache
                .load_prefix_from_keydb::<Vec<WbListRouteDto>>(wb_prefix)
                .await;
            if !wb_keydb.is_empty() {
                cache
                    .atomic_reload_prefix(wb_prefix, wb_keydb.clone())
                    .await;
                let wb_by_etag: HashMap<String, Vec<WbListRouteDto>> = wb_keydb
                    .into_iter()
                    .filter_map(|(key, list)| {
                        let prefix = format!("{}:", wb_prefix);
                        key.strip_prefix(&prefix)
                            .map(|etag| (etag.to_string(), list))
                    })
                    .collect();
                let n = wb_by_etag.len();
                wb_route_cache::fill_memory_from_reload(wb_by_etag);
                tracing::info!(
                    from_keydb = n,
                    "[Cache] TollFeeList WB list loaded from KeyDB (DB failed, fallback)"
                );
            }
        }
    }

    if let Ok(rows) = ex_res {
        let ex_by_etag: HashMap<String, Vec<ExListItemDto>> = group_by_etag_ex(rows);
        let n = ex_by_etag.len();
        let ex_cache_data: Vec<(String, Vec<ExListItemDto>)> = ex_by_etag
            .iter()
            .map(|(etag, list)| (ex_list_key(etag), list.clone()))
            .collect();
        cache.atomic_reload_prefix(ex_prefix, ex_cache_data).await;
        ex_list_cache::fill_memory_from_reload(ex_by_etag);
        if load_from_keydb {
            tracing::info!(
                count = n,
                "[Cache] TollFeeList Ex list loaded from DB, synced to KeyDB"
            );
        } else {
            tracing::info!(count = n, "[Cache] TollFeeList Ex list loaded from DB");
        }
    } else {
        if let Err(e) = &ex_res {
            if !is_table_or_view_not_found(e) {
                tracing::warn!(error = ?e, "[Cache] TollFeeList Ex list load from DB failed");
            }
        }
        if load_from_keydb {
            let ex_keydb = cache
                .load_prefix_from_keydb::<Vec<ExListItemDto>>(ex_prefix)
                .await;
            if !ex_keydb.is_empty() {
                cache
                    .atomic_reload_prefix(ex_prefix, ex_keydb.clone())
                    .await;
                let ex_by_etag: HashMap<String, Vec<ExListItemDto>> = ex_keydb
                    .into_iter()
                    .filter_map(|(key, list)| {
                        let prefix = format!("{}:", ex_prefix);
                        key.strip_prefix(&prefix)
                            .map(|etag| (etag.to_string(), list))
                    })
                    .collect();
                let n = ex_by_etag.len();
                ex_list_cache::fill_memory_from_reload(ex_by_etag);
                tracing::info!(
                    from_keydb = n,
                    "[Cache] TollFeeList Ex list loaded from KeyDB (DB failed, fallback)"
                );
            }
        }
    }

    if let Ok(rows) = ex_price_res {
        let ex_price_by_etag: HashMap<String, Vec<ExPriceListItemDto>> =
            group_by_etag_ex_price(rows);
        let n = ex_price_by_etag.len();
        let ex_price_cache_data: Vec<(String, Vec<ExPriceListItemDto>)> = ex_price_by_etag
            .iter()
            .map(|(etag, list)| (ex_price_list_key(etag), list.clone()))
            .collect();
        cache
            .atomic_reload_prefix(ex_price_prefix, ex_price_cache_data)
            .await;
        ex_price_list_cache::fill_memory_from_reload(ex_price_by_etag);
        if load_from_keydb {
            tracing::info!(
                count = n,
                "[Cache] TollFeeList Ex price list loaded from DB, synced to KeyDB"
            );
        } else {
            tracing::info!(
                count = n,
                "[Cache] TollFeeList Ex price list loaded from DB"
            );
        }
    } else {
        if let Err(e) = &ex_price_res {
            if !is_table_or_view_not_found(e) {
                tracing::warn!(
                    error = ?e,
                    "[Cache] TollFeeList Ex price list load from DB failed"
                );
            }
        }
        if load_from_keydb {
            let ex_price_keydb = cache
                .load_prefix_from_keydb::<Vec<ExPriceListItemDto>>(ex_price_prefix)
                .await;
            if !ex_price_keydb.is_empty() {
                cache
                    .atomic_reload_prefix(ex_price_prefix, ex_price_keydb.clone())
                    .await;
                let ex_price_by_etag: HashMap<String, Vec<ExPriceListItemDto>> = ex_price_keydb
                    .into_iter()
                    .filter_map(|(key, list)| {
                        let prefix = format!("{}:", ex_price_prefix);
                        key.strip_prefix(&prefix)
                            .map(|etag| (etag.to_string(), list))
                    })
                    .collect();
                let n = ex_price_by_etag.len();
                ex_price_list_cache::fill_memory_from_reload(ex_price_by_etag);
                tracing::info!(
                    from_keydb = n,
                    "[Cache] TollFeeList Ex price list loaded from KeyDB (DB failed, fallback)"
                );
            }
        }
    }
}

fn group_by_etag_wb(rows: Vec<WbListRouteDto>) -> HashMap<String, Vec<WbListRouteDto>> {
    let mut by_etag: HashMap<String, Vec<WbListRouteDto>> = HashMap::new();
    for dto in rows {
        let etag = normalize_etag(dto.etag.as_deref().unwrap_or(""));
        if etag.is_empty() {
            continue;
        }
        by_etag.entry(etag).or_default().push(dto);
    }
    by_etag
}

fn group_by_etag_ex(rows: Vec<ExListItemDto>) -> HashMap<String, Vec<ExListItemDto>> {
    let mut by_etag: HashMap<String, Vec<ExListItemDto>> = HashMap::new();
    for dto in rows {
        let etag = normalize_etag(dto.etag.as_deref().unwrap_or(""));
        if etag.is_empty() {
            continue;
        }
        by_etag.entry(etag).or_default().push(dto);
    }
    by_etag
}

fn group_by_etag_ex_price(
    rows: Vec<ExPriceListItemDto>,
) -> HashMap<String, Vec<ExPriceListItemDto>> {
    let mut by_etag: HashMap<String, Vec<ExPriceListItemDto>> = HashMap::new();
    for dto in rows {
        let etag = normalize_etag(dto.etag.as_deref().unwrap_or(""));
        if etag.is_empty() {
            continue;
        }
        by_etag.entry(etag).or_default().push(dto);
    }
    by_etag
}

/// WB_LIST: cột STATION_ID, CYCLE_ID, ETAG, PLATE, ... Chỉ áp dụng WB khi cycle_id của toll_stage (tollA,tollB) = CYCLE_ID trong WB (nếu có).
/// Thời gian so sánh luôn UTC (truyền từ ứng dụng).
async fn load_wb_list_from_db(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
) -> Result<Vec<WbListRouteDto>, DbError> {
    let now_utc = crate::utils::now_utc_db_string();
    let now_utc_sql = crate::db::format_sql_string(&now_utc);

    task::spawn_blocking(move || {
        let query = format!(
            r#"
            SELECT e.station_id, e.cycle_id, NVL(TRIM(e.item_type), '') AS item_type,
                   e.etag, e.plate, e.plate_type, e.effect_date, e.expire_date,
                   e.type_station, e.boo, e.vehicle_type
            FROM CRM_OWNER.WB_LIST e
            WHERE e.STATUS = '11'
              AND e.EFFECT_DATE <= TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS') + 1/24
              AND (e.EXPIRE_DATE IS NULL OR e.EXPIRE_DATE >= TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS') - 1/24)
            ORDER BY e.station_id, e.etag
            "#,
            now_utc_sql,
            now_utc_sql
        );
        let conn = get_connection_with_retry(db_pool.as_ref())?;
        let cursor_opt = conn.execute(&query, (), None)?;
        let mut out = Vec::new();
        if let Some(mut cursor) = cursor_opt {
            while let Ok(Some(mut row)) = cursor.next_row() {
                let station_id = get_nullable_i64(&mut row, 1)?.unwrap_or(0);
                let cycle_id = get_nullable_i64(&mut row, 2)?;
                let item_type = get_nullable_string(&mut row, 3)?.unwrap_or_default();
                let etag = get_nullable_string(&mut row, 4)?;
                let plate = get_nullable_string(&mut row, 5)?;
                let plate_type = get_nullable_string(&mut row, 6)?;
                let effect_date = get_nullable_datetime_string(&mut row, 7)?;
                let expire_date = get_nullable_datetime_string(&mut row, 8)?;
                let type_station = get_nullable_string(&mut row, 9)?;
                let boo = get_nullable_string(&mut row, 10)?;
                let vehicle_type = get_nullable_string(&mut row, 11)?;
                out.push(WbListRouteDto {
                    toll_id: station_id,
                    cycle_id,
                    item_type,
                    etag,
                    plate,
                    plate_type,
                    effect_date,
                    expire_date,
                    type_station,
                    boo,
                    vehicle_type,
                });
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| DbError::ExecutionError(format!("wb_list task: {}", e)))?
}

/// EX_LIST: STATION_ID (0 toàn quốc; toll_id trạm mở; stage_id trạm kín), TYPE_STATION (0/1/2), STATUS '11', EFFECT_DATE/EXPIRE_DATE.
/// Thời gian so sánh luôn UTC (truyền từ ứng dụng).
async fn load_ex_list_from_db(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
) -> Result<Vec<ExListItemDto>, DbError> {
    let now_utc = crate::utils::now_utc_db_string();
    let now_utc_sql = crate::db::format_sql_string(&now_utc);

    task::spawn_blocking(move || {
        let query = format!(
            r#"
            SELECT e.station_id,
                   NVL(TRIM(e.vehicle_type_boo), NVL(TRIM(e.vehicle_type), '')) AS vehicle_type,
                   e.etag, e.plate, e.effect_date, e.expire_date,
                   e.vehicle_type_profile, e.boo, e.vehicle_type_boo,
                   e.type_station
            FROM CRM_OWNER.EX_LIST e
            WHERE e.STATUS = '11'
              AND e.EFFECT_DATE <= TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS') + 1/24
              AND (e.EXPIRE_DATE IS NULL OR e.EXPIRE_DATE >= TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS') - 1/24)
            ORDER BY e.station_id, e.etag
            "#,
            now_utc_sql,
            now_utc_sql
        );
        let conn = get_connection_with_retry(db_pool.as_ref())?;
        let cursor_opt = conn.execute(&query, (), None)?;
        let mut out = Vec::new();
        if let Some(mut cursor) = cursor_opt {
            while let Ok(Some(mut row)) = cursor.next_row() {
                let station_id = get_i64(&mut row, 1)?;
                let vehicle_type = get_nullable_string(&mut row, 2)?.unwrap_or_default();
                let etag = get_nullable_string(&mut row, 3)?;
                let plate = get_nullable_string(&mut row, 4)?;
                let effect_date = get_nullable_datetime_string(&mut row, 5)?;
                let expire_date = get_nullable_datetime_string(&mut row, 6)?;
                let vehicle_type_profile = get_nullable_string(&mut row, 7)?;
                let boo = get_nullable_string(&mut row, 8)?;
                let vehicle_type_boo = get_nullable_string(&mut row, 9)?;
                let type_station = get_nullable_string(&mut row, 10)?;
                out.push(ExListItemDto {
                    station_id,
                    vehicle_type,
                    etag,
                    plate,
                    effect_date,
                    expire_date,
                    vehicle_type_profile,
                    boo,
                    vehicle_type_boo,
                    type_station,
                });
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| DbError::ExecutionError(format!("ex_list task: {}", e)))?
}

/// EX_PRICE_LIST: STATION_ID (0 toàn quốc; toll_id trạm mở; stage_id trạm kín), TYPE_STATION (0/1/2), STATUS '11', PRICE_TICKET_TYPE.
/// Thời gian so sánh luôn UTC (truyền từ ứng dụng).
async fn load_ex_price_list_from_db(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
) -> Result<Vec<ExPriceListItemDto>, DbError> {
    let now_utc = crate::utils::now_utc_db_string();
    let now_utc_sql = crate::db::format_sql_string(&now_utc);

    task::spawn_blocking(move || {
        let query = format!(
            r#"
            SELECT e.station_id, NVL(TRIM(e.price_ticket_type), '0') AS price_ticket_type,
                   e.etag, e.plate, e.vehicle_type, e.effect_date, e.expire_date,
                   e.boo, e.vehicle_type_boo,
                   e.type_station
            FROM CRM_OWNER.EX_PRICE_LIST e
            WHERE e.STATUS = '11'
              AND e.EFFECT_DATE <= TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS') + 1/24
              AND (e.EXPIRE_DATE IS NULL OR e.EXPIRE_DATE >= TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS') - 1/24)
            ORDER BY e.station_id, e.etag
            "#,
            now_utc_sql,
            now_utc_sql
        );
        let conn = get_connection_with_retry(db_pool.as_ref())?;
        let cursor_opt = conn.execute(&query, (), None)?;
        let mut out = Vec::new();
        if let Some(mut cursor) = cursor_opt {
            while let Ok(Some(mut row)) = cursor.next_row() {
                let station_id = get_i64(&mut row, 1)?;
                let price_ticket_type =
                    get_nullable_string(&mut row, 2)?.unwrap_or_else(|| "0".to_string());
                let etag = get_nullable_string(&mut row, 3)?;
                let plate = get_nullable_string(&mut row, 4)?;
                let vehicle_type = get_nullable_string(&mut row, 5)?;
                let effect_date = get_nullable_datetime_string(&mut row, 6)?;
                let expire_date = get_nullable_datetime_string(&mut row, 7)?;
                let boo = get_nullable_string(&mut row, 8)?;
                let vehicle_type_boo = get_nullable_string(&mut row, 9)?;
                let type_station = get_nullable_string(&mut row, 10)?;
                out.push(ExPriceListItemDto {
                    station_id,
                    price_ticket_type,
                    etag,
                    plate,
                    vehicle_type,
                    effect_date,
                    expire_date,
                    boo,
                    vehicle_type_boo,
                    type_station,
                });
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| DbError::ExecutionError(format!("ex_price_list task: {}", e)))?
}
