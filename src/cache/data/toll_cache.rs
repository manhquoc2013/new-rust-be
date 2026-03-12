//! Toll station cache (toll), get_toll_cache, load from DB.

use std::sync::Arc;

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::config::cache_prefix::CachePrefix;
use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::{
    get_i32, get_nullable_datetime_string, get_nullable_i32, get_nullable_i64, get_nullable_string,
    DbError,
};
use crate::models::TollCache::{replace_all_tolls, TOLL};
use odbc_api::Cursor;
use r2d2::Pool;
use tokio::task;

/// Load cache TOLL. Prefer DB; khi startup nếu DB failed hoặc pool None thì fallback KeyDB (khi `load_from_keydb` true). Reload luôn từ DB.
/// `pool_opt`: Option vì RATING có thể chưa sẵn sàng lúc khởi động (init DB lỗi); khi None to omit DB.
pub async fn get_toll_cache(
    pool_opt: Option<Arc<Pool<OdbcConnectionManager>>>,
    cache: Arc<CacheManager>,
    load_from_keydb: bool,
) {
    let result = match pool_opt {
        Some(pool) => get_active_tolls(pool).await,
        None => {
            tracing::warn!(
                "[Cache] Toll skip DB (pool not available), will use KeyDB fallback if enabled"
            );
            Err(crate::db::error::DbError::ExecutionError(
                "DB pool not available".to_string(),
            ))
        }
    };
    match result {
        Ok(tolls) => {
            let count = tolls.len();
            replace_all_tolls(tolls.clone());
            let cache_data: Vec<(String, TOLL)> = tolls
                .into_iter()
                .map(|toll| {
                    let key = cache.gen_key(CachePrefix::Toll, &[&toll.toll_id]);
                    (key, toll)
                })
                .collect();
            cache
                .atomic_reload_prefix(CachePrefix::Toll.as_str(), cache_data)
                .await;
            if load_from_keydb {
                tracing::info!(count, "[Cache] Toll loaded from DB, synced to KeyDB");
            } else {
                tracing::info!(count, "[Cache] Toll loaded from DB");
            }
        }
        Err(e) => {
            tracing::error!(error = ?e, "[Cache] Toll load from DB failed");
            if !load_from_keydb {
                return;
            }
            let prefix = CachePrefix::Toll.as_str();
            let keydb_loaded = cache.load_prefix_from_keydb::<TOLL>(prefix).await;
            if keydb_loaded.is_empty() {
                tracing::warn!("[Cache] Toll fallback KeyDB empty, keeping empty");
                return;
            }
            cache
                .atomic_reload_prefix(prefix, keydb_loaded.clone())
                .await;
            let all_tolls: Vec<TOLL> = keydb_loaded.into_iter().map(|(_, t)| t).collect();
            replace_all_tolls(all_tolls.clone());
            tracing::info!(
                count = all_tolls.len(),
                "[Cache] Toll loaded from KeyDB (DB failed, fallback)"
            );
        }
    }
}

/// Lấy một TOLL từ DB theo toll_id (dùng cho fallback khi miss in-memory).
pub async fn get_toll_from_db(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    toll_id: i32,
) -> Result<Option<TOLL>, DbError> {
    let pool = db_pool.clone();
    task::spawn_blocking(move || get_toll_by_id_blocking(pool, toll_id))
        .await
        .map_err(|e| {
            DbError::ExecutionError(format!(
                "[TollCache] get_toll_from_db spawn_blocking: {}",
                e
            ))
        })?
}

fn get_toll_by_id_blocking(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    toll_id: i32,
) -> Result<Option<TOLL>, DbError> {
    use crate::db::format_sql_value;
    let query = format!(
        r#"SELECT TOLL_ID, TOLL_NAME, ADDRESS, PROVINCE, DISTRICT, PRECINCT, TOLL_TYPE,
           TELL_NUMBER, FAX, STATUS, TOLL_CODE, TOLL_NAME_SEARCH, RATE_TYPE_PARKING, STATUS_COMMERCIAL,
           CALCULATION_TYPE, TCOC_VERSION_ID, SYNTAX_CODE, VEHICLE_TYPE_PROFILE, ALLOW_SAME_INOUT,
           ALLOW_SAME_INOUT_TIME, TO_CHAR(LATCH_HOUR, 'YYYY-MM-DD HH24:MI:SS'), BOO, CLOSE_TIME_CODE, CALC_TYPE, SEQUENCE_TYPE, SEQUENCE_APPLY
           FROM RATING_OWNER.TOLL WHERE STATUS = '1' AND TOLL_ID = {}"#,
        format_sql_value(&toll_id)
    );
    let conn = get_connection_with_retry(db_pool.as_ref())?;
    let cursor_result = conn.execute(&query, (), None)?;
    match cursor_result {
        Some(mut cursor) => match cursor.next_row() {
            Ok(Some(mut row)) => {
                let toll = row_to_toll(&mut row)?;
                Ok(Some(toll))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(DbError::ExecutionError(format!(
                "Error fetching TOLL row: {}",
                e
            ))),
        },
        None => Ok(None),
    }
}

async fn get_active_tolls(db_pool: Arc<Pool<OdbcConnectionManager>>) -> Result<Vec<TOLL>, DbError> {
    task::spawn_blocking(move || {
        let query =
            r#"SELECT TOLL_ID, TOLL_NAME, ADDRESS, PROVINCE, DISTRICT, PRECINCT, TOLL_TYPE,
               TELL_NUMBER, FAX, STATUS, TOLL_CODE, TOLL_NAME_SEARCH, RATE_TYPE_PARKING, STATUS_COMMERCIAL,
               CALCULATION_TYPE, TCOC_VERSION_ID, SYNTAX_CODE, VEHICLE_TYPE_PROFILE, ALLOW_SAME_INOUT,
               ALLOW_SAME_INOUT_TIME, TO_CHAR(LATCH_HOUR, 'YYYY-MM-DD HH24:MI:SS'), BOO, CLOSE_TIME_CODE, CALC_TYPE, SEQUENCE_TYPE, SEQUENCE_APPLY
               FROM RATING_OWNER.TOLL WHERE STATUS = '1'"#;
        let mut result = Vec::new();
        let conn = get_connection_with_retry(db_pool.as_ref())?;
        let cursor_result = conn.execute(query, (), None)?;
        if let Some(mut cursor) = cursor_result {
            loop {
                match cursor.next_row() {
                    Ok(Some(mut row)) => {
                        let toll = row_to_toll(&mut row)?;
                        result.push(toll);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return Err(DbError::ExecutionError(format!("Error fetching TOLL row: {}", e)));
                    }
                }
            }
        }
        Ok(result)
    })
    .await
    .map_err(|e| DbError::ExecutionError(format!("[TollCache] get_active_tolls spawn_blocking: {}", e)))?
}

/// Map một row (column index 1-based) sang TOLL.
/// Trường Option trong TOLL dùng get_nullable_*; trường String có thể NULL thì get_nullable_string + unwrap_or_default.
fn row_to_toll(row: &mut odbc_api::CursorRow) -> Result<TOLL, DbError> {
    let toll_id = get_i32(row, 1)?;
    let toll_name = get_nullable_string(row, 2)?.unwrap_or_default();
    let address = get_nullable_string(row, 3)?.unwrap_or_default();
    let province = get_nullable_string(row, 4)?.unwrap_or_default();
    let district = get_nullable_string(row, 5)?.unwrap_or_default();
    let precinct = get_nullable_string(row, 6)?.unwrap_or_default();
    let toll_type = get_nullable_string(row, 7)?.unwrap_or_default();
    let tell_number = get_nullable_string(row, 8)?.unwrap_or_default();
    let fax = get_nullable_string(row, 9)?.unwrap_or_default();
    let status = get_nullable_string(row, 10)?.unwrap_or_default();
    let toll_code = get_nullable_string(row, 11)?.unwrap_or_default();
    let toll_name_search = get_nullable_string(row, 12)?.unwrap_or_default();
    let rate_type_parking = get_nullable_string(row, 13)?.unwrap_or_default();
    let status_commercial = get_nullable_string(row, 14)?.unwrap_or_default();
    let calculation_type = get_nullable_string(row, 15)?.unwrap_or_default();
    let tcoc_version_id = get_nullable_i32(row, 16)?;
    let syntax_code = get_nullable_string(row, 17)?.unwrap_or_default();
    let vehicle_type_profile = get_nullable_string(row, 18)?.unwrap_or_default();
    let allow_same_inout = get_nullable_string(row, 19)?.unwrap_or_else(|| "0".to_string());
    let allow_same_inout_time = get_nullable_i64(row, 20)?.unwrap_or(0);
    let latch_hour = get_nullable_datetime_string(row, 21)?;
    let boo = get_nullable_string(row, 22)?.unwrap_or_else(|| "1".to_string());
    let close_time_code = get_nullable_string(row, 23)?.unwrap_or_default();
    let calc_type = get_nullable_string(row, 24)?.unwrap_or_default();
    let sequence_type = get_nullable_string(row, 25)?.unwrap_or_else(|| "INT".to_string());
    let sequence_apply = get_nullable_string(row, 26)?.unwrap_or_else(|| "ALL".to_string());

    Ok(TOLL {
        toll_id,
        toll_name,
        address,
        province,
        district,
        precinct,
        toll_type,
        tell_number,
        fax,
        status,
        toll_code,
        toll_name_search,
        rate_type_parking,
        status_commercial,
        calculation_type,
        tcoc_version_id,
        syntax_code,
        vehicle_type_profile,
        allow_same_inout,
        allow_same_inout_time,
        latch_hour,
        boo,
        close_time_code,
        calc_type,
        sequence_type,
        sequence_apply,
    })
}
