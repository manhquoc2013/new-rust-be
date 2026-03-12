//! Cache subscription history theo etag (giống Java: getCacheSubscriptionHistoryFull().get(etag)).
//! Nguồn: RATING_OWNER.SUBSCRIPTION_HISTORY join CRM_OWNER.SUBSCRIBER, CRM_OWNER.ETAG.
//! Dùng để lấy danh sách stage_id đã trả bởi subscription cho etag → truyền vào calculate_toll_fee làm subscription_stage_ids.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use odbc_api::Cursor;
use r2d2::Pool;
use tokio::task;

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::config::cache_prefix::CachePrefix;
use crate::cache::data::dto::subscription_history_dto::SubscriptionHistoryDto;
use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::{get_i64, get_nullable_datetime_string, get_nullable_string, DbError};

fn subscription_history_key(etag: &str) -> String {
    format!("{}:{}", CachePrefix::SubscriptionHistory.as_str(), etag)
}

/// Lấy subscription history và tập stage_id trong một lần đọc cache (tối ưu cho handler tính phí).
/// Trả về (history, stage_ids); cache miss trả (None, HashSet::new()).
pub async fn get_subscription_history_and_stage_ids_for_etag(
    cache: Arc<CacheManager>,
    etag: &str,
) -> (Option<Vec<SubscriptionHistoryDto>>, HashSet<i64>) {
    let etag = etag.trim();
    if etag.is_empty() {
        return (None, HashSet::new());
    }
    let list = cache
        .get::<Vec<SubscriptionHistoryDto>>(&subscription_history_key(etag))
        .await;
    let stage_ids = list
        .as_ref()
        .map(|l| l.iter().map(|d| d.stage_id).collect())
        .unwrap_or_default();
    (list, stage_ids)
}

/// Load cache SubscriptionHistory. Prefer DB; khi startup nếu DB failed hoặc pool None thì fallback KeyDB (khi `load_from_keydb` true). Reload luôn từ DB.
/// `pool_opt`: Option vì RATING có thể chưa sẵn sàng lúc khởi động; khi None to omit DB.
pub async fn get_subscription_history_cache(
    pool_opt: Option<Arc<Pool<OdbcConnectionManager>>>,
    cache: Arc<CacheManager>,
    load_from_keydb: bool,
) {
    let result = match pool_opt {
        Some(pool) => load_subscription_history_from_db(pool).await,
        None => {
            tracing::warn!("[Cache] SubscriptionHistory skip DB (pool not available), will use KeyDB fallback if enabled");
            Err(crate::db::error::DbError::ExecutionError(
                "DB pool not available".to_string(),
            ))
        }
    };
    match result {
        Ok(by_etag) => {
            let prefix = CachePrefix::SubscriptionHistory.as_str();
            let cache_data: Vec<(String, Vec<SubscriptionHistoryDto>)> = by_etag
                .iter()
                .filter(|(etag, _)| !etag.is_empty())
                .map(|(etag, list)| (subscription_history_key(etag), list.clone()))
                .collect();
            let n = cache_data.len();
            cache.atomic_reload_prefix(prefix, cache_data).await;
            if load_from_keydb {
                tracing::info!(
                    count = n,
                    "[Cache] SubscriptionHistory loaded from DB, synced to KeyDB"
                );
            } else {
                tracing::info!(count = n, "[Cache] SubscriptionHistory loaded from DB");
            }
        }
        Err(e) => {
            tracing::warn!(error = ?e, "[Cache] SubscriptionHistory load from DB failed");
            if !load_from_keydb {
                return;
            }
            let prefix = CachePrefix::SubscriptionHistory.as_str();
            let keydb_loaded = cache
                .load_prefix_from_keydb::<Vec<SubscriptionHistoryDto>>(prefix)
                .await;
            if keydb_loaded.is_empty() {
                tracing::warn!("[Cache] SubscriptionHistory fallback KeyDB empty, keeping empty");
                return;
            }
            let n = keydb_loaded.len();
            cache.atomic_reload_prefix(prefix, keydb_loaded).await;
            tracing::info!(
                from_keydb = n,
                "[Cache] SubscriptionHistory loaded from KeyDB (DB failed, fallback)"
            );
        }
    }
}

/// Query giống Java MedRatingPriceBean.loadSubscriptionHistoryCache: SUBSCRIPTION_HISTORY join TOLL_STAGE, PRICE, SUBSCRIBER, ETAG.
/// Thời gian so sánh luôn UTC (truyền từ ứng dụng).
async fn load_subscription_history_from_db(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
) -> Result<HashMap<String, Vec<SubscriptionHistoryDto>>, DbError> {
    let now_utc = crate::utils::now_utc_db_string();
    let now_utc_sql = crate::db::format_sql_string(&now_utc);

    let join_handle = task::spawn_blocking(move || {
        let query = format!(
            r#"
            SELECT etag.etag_number, sh.stage_id, sh.subcription_hist_id, sh.price_type, TS.BOO,
                   TO_CHAR(sh.start_date, 'YYYY-MM-DD HH24:MI:SS'), TO_CHAR(sh.end_date, 'YYYY-MM-DD HH24:MI:SS'),
                   P.PRICE_ID, BT.BOT_ID
            FROM RATING_OWNER.SUBSCRIPTION_HISTORY SH
            INNER JOIN RATING_OWNER.TOLL_STAGE TS ON SH.STAGE_ID = TS.STAGE_ID
            INNER JOIN RATING_OWNER.PRICE P ON SH.PRICE_ID = P.PRICE_ID
            LEFT JOIN RATING_OWNER.BOT_TOLL BT ON BT.STAGE_ID = P.STAGE_ID
            INNER JOIN CRM_OWNER.SUBSCRIBER SUB ON SH.SUBSCRIBER_ID = SUB.SUBSCRIBER_ID
            INNER JOIN CRM_OWNER.ETAG ETAG ON SUB.ETAG_ID = ETAG.ETAG_ID
            WHERE SH.TOLL_TYPE = 'C'
              AND SH.START_DATE < TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS')
              AND (SH.END_DATE >= TRUNC(TO_DATE({}, 'YYYY-MM-DD HH24:MI:SS')) OR SH.END_DATE IS NULL)
              AND SH.STATUS = 1
            ORDER BY etag.etag_number, sh.stage_id
            "#,
            now_utc_sql, now_utc_sql
        );
        let conn = get_connection_with_retry(db_pool.as_ref())?;
        let cursor_opt = conn.execute(&query, (), None)?;
        let mut by_etag: HashMap<String, Vec<SubscriptionHistoryDto>> = HashMap::new();
        if let Some(mut cursor) = cursor_opt {
            while let Ok(Some(mut row)) = cursor.next_row() {
                let etag = get_nullable_string(&mut row, 1)?
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                let stage_id = get_i64(&mut row, 2)?;
                let subscription_id = get_i64(&mut row, 3)?;
                let price_type = get_nullable_string(&mut row, 4)?;
                let stage_boo = get_nullable_string(&mut row, 5)?;
                let start_date = get_nullable_datetime_string(&mut row, 6)?;
                let end_date = get_nullable_datetime_string(&mut row, 7)?;
                let price_id = crate::db::get_nullable_i64(&mut row, 8).ok().flatten();
                let bot_id = crate::db::get_nullable_i64(&mut row, 9).ok().flatten();
                by_etag
                    .entry(etag)
                    .or_default()
                    .push(SubscriptionHistoryDto {
                        stage_id,
                        subscription_id,
                        price_type,
                        price_id,
                        bot_id,
                        stage_boo,
                        start_date,
                        end_date,
                    });
            }
        }
        Ok(by_etag)
    });
    join_handle
        .await
        .map_err(|e| DbError::ExecutionError(format!("subscription_history task: {}", e)))?
}
