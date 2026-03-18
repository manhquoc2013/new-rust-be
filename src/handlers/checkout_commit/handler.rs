//! BECT checkout commit logic: validate stage, update ACCOUNT_TRANSACTION and TRANSPORT_TRANSACTION_STAGE, clear ETDR.

use crate::cache::config::cache_manager::CacheManager;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::configs::rating_db::RATING_DB;
use crate::constants::account_transaction;
use crate::constants::fe;
use crate::db::repositories::{
    allow_commit_or_rollback, get_account_transaction_for_commit,
    update_account_transaction_commit_status,
};
use crate::handlers::checkin::common::{
    get_best_pending_from_sync_or_main_bect, merge_latest_checkin_etdr,
};
use crate::handlers::commit::common::invalidate_tcd_rating_cache;
use crate::models::bect_messages::CHECKOUT_COMMIT_BOO;
use crate::models::ETDR::{
    clear_etdr_after_transaction_complete, get_etdr_cache, get_latest_checkin_by_etag,
};
use crate::services::service::Service;
use crate::services::TransportTransactionStageService;
use crate::utils::{normalize_etag, now_utc_db_string};
use r2d2::Pool;
use std::error::Error;
use std::sync::Arc;

/// Runs BECT checkout commit: resolve ETDR/stage, update account and stage, clear ETDR. Returns status for CHECKOUT_COMMIT_BOO_RESP.
pub async fn process_checkout_commit_bect(
    req: &CHECKOUT_COMMIT_BOO,
    _conn_id: i32,
    _db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
) -> Result<i32, Box<dyn Error>> {
    let etag_norm = normalize_etag(&req.etag);

    let transport_trans_id = if req.ticket_in_id != 0 {
        req.ticket_in_id
    } else {
        let cache_opt = get_latest_checkin_by_etag(&etag_norm).await;
        let db_opt = get_best_pending_from_sync_or_main_bect(&etag_norm).await;
        let etdr_opt = merge_latest_checkin_etdr(cache_opt, db_opt);
        etdr_opt.map(|e| e.ticket_id).unwrap_or(0)
    };

    if transport_trans_id == 0 {
        return Ok(fe::NOT_FOUND_ROUTE_TRANSACTION);
    }

    let etdr_opt = get_etdr_cache().get_by_ticket_id(transport_trans_id);
    let etdr = match etdr_opt {
        Some(ref e) => e.clone(),
        None => {
            let cache_opt = get_latest_checkin_by_etag(&etag_norm).await;
            let db_opt = get_best_pending_from_sync_or_main_bect(&etag_norm).await;
            match merge_latest_checkin_etdr(cache_opt, db_opt) {
                Some(e) if e.ticket_id == transport_trans_id => e,
                _ => return Ok(fe::NOT_FOUND_ROUTE_TRANSACTION),
            }
        }
    };

    if etdr.checkout_datetime == 0
        || etdr.checkout_commit_datetime != 0
        || etdr.checkin_commit_datetime == 0
    {
        return Ok(fe::NOT_FOUND_ROUTE_TRANSACTION);
    }

    let transport_service = TransportTransactionStageService::default();
    let stage = match transport_service.get_by_id(transport_trans_id).await {
        Ok(Some(s)) => s,
        Ok(None) => return Ok(fe::NOT_FOUND_ROUTE_TRANSACTION),
        Err(_) => return Ok(fe::NOT_FOUND_ROUTE_TRANSACTION),
    };

    let now = now_utc_db_string();

    if let Some(account_trans_id) = stage.account_trans_id {
        if account_trans_id > 0 {
            let pool = RATING_DB.clone();
            let atid = account_trans_id;
            let commit_data = tokio::task::spawn_blocking(move || {
                get_account_transaction_for_commit(&pool, atid)
            })
            .await
            .unwrap_or(Ok(None))
            .unwrap_or(None);

            if let Some(data) = commit_data {
                if allow_commit_or_rollback(data.autocommit, data.autocommit_status) {
                    let new_balance = data.old_balance - data.amount;
                    let new_avail = data.old_available_bal - data.amount;
                    let pool_up = RATING_DB.clone();
                    let now_for_commit = now.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        update_account_transaction_commit_status(
                            &pool_up,
                            atid,
                            account_transaction::AUTOCOMMIT_STATUS_SUCCESS,
                            &now_for_commit,
                            new_balance,
                            new_avail,
                            None,
                            None,
                        )
                    })
                    .await;
                }
            }
        }
    }

    let _ = transport_service
        .update_checkout_commit(
            transport_trans_id,
            &now,
            0,
            &now,
            "COMMIT",
            Some("1"),
            None,
            None,
            None,
            Some("1"),
            stage.checkout_plate.as_deref(),
            Some(0),
        )
        .await;

    clear_etdr_after_transaction_complete(&etag_norm, Some(transport_trans_id)).await;
    invalidate_tcd_rating_cache(cache.as_ref(), transport_trans_id, true).await;

    Ok(0)
}
