//! BECT checkout reserve logic (exit station): resolve ETDR, calculate fee, update stage and TCD, return CHECKOUT_RESERVE_BOO_RESP.

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::data::subscription_history_cache::get_subscription_history_and_stage_ids_for_etag;
use crate::cache::data::toll_fee_list_cache::get_toll_fee_list_context;
use crate::configs::config::Config;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::configs::rating_db::RATING_DB;
use crate::constants::{bect, fe};
use crate::db::repositories::account_repository::get_account_for_charge;
use crate::db::repositories::TransportTransStageTcd;
use crate::db::repositories::{
    insert_account_transaction_for_bect_checkout, InsertAccountTransactionParams,
};
use crate::handlers::checkin::common::{
    get_best_pending_from_sync_or_main_bect, get_price_ticket_type_from_rating_detail,
    merge_latest_checkin_etdr, price_ticket_type_for_etdr_from_rating_details,
    ticket_type_i32_to_str, PLATE_EMPTY_SENTINEL,
};
use crate::handlers::commit::common::{set_tcd_rating_cache, vdtc_rating_details_to_boo};
use crate::models::bect_messages::CHECKOUT_RESERVE_BOO;
use crate::models::ETDR::{
    get_latest_checkin_by_etag, save_bect_tcd_list_with_lock, save_etdr, set_pending_tcd_bect,
};
use crate::services::toll_fee::TollFeeService;
use crate::services::transport_transaction_stage_service::TransportTransactionStageService;
use crate::utils::{bect_skip_account_check, normalize_etag};
use r2d2::Pool;
use std::error::Error;
use std::sync::Arc;

/// Runs BECT checkout reserve: resolve ETDR by etag, calculate toll fee, update TRANSPORT_TRANSACTION_STAGE and TCD, return status and ticket ids for CHECKOUT_RESERVE_BOO_RESP.
pub async fn process_checkout_reserve_bect(
    req: &CHECKOUT_RESERVE_BOO,
    _conn_id: i32,
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
) -> Result<(i32, i64, i64), Box<dyn Error>> {
    let etag_query = normalize_etag(&req.etag);
    let cache_opt = get_latest_checkin_by_etag(&etag_query).await;
    let db_opt = get_best_pending_from_sync_or_main_bect(&etag_query).await;
    let etdr_opt = merge_latest_checkin_etdr(cache_opt, db_opt);

    match etdr_opt {
        Some(mut etdr) => {
            if etdr.toll_out != 0
                || etdr.checkout_datetime != 0
                || etdr.checkin_commit_datetime == 0
                || etdr.checkout_commit_datetime != 0
                || etdr.checkin_datetime == 0
            {
                return Ok((
                    fe::NOT_FOUND_ROUTE_TRANSACTION,
                    etdr.ticket_id,
                    etdr.ticket_id,
                ));
            }

            let etag_str = normalize_etag(&req.etag);
            let (list_ctx, (subscription_history, subscription_stage_set)) = tokio::join!(
                get_toll_fee_list_context(db_pool.clone(), cache.clone(), &etag_str),
                get_subscription_history_and_stage_ids_for_etag(cache.clone(), &etag_str),
            );

            let trans_datetime = if etdr.checkout_datetime > 0 {
                crate::utils::epoch_to_datetime_utc(etdr.checkout_datetime).map(|dt| dt.naive_utc())
            } else {
                Some(chrono::Utc::now().naive_utc())
            };

            let (trans_amount, rating_details) = match TollFeeService::calculate_toll_fee(
                db_pool.clone(),
                cache.clone(),
                etdr.station_id,
                req.station_out,
                etdr.vehicle_type.parse::<i32>().unwrap_or(1),
                &ticket_type_i32_to_str(etdr.ticket_type),
                (etdr.sub_id != 0).then_some(etdr.sub_id as i64),
                list_ctx.as_ref(),
                None,
                Some(&subscription_stage_set),
                subscription_history.as_deref(),
                trans_datetime,
            )
            .await
            {
                Ok(t) => t,
                Err(_e) => {
                    if Config::get().allow_zero_fee_when_no_price_or_route {
                        (0, vec![])
                    } else {
                        return Ok((bect::NOT_FOUND_PRICE_INFO, etdr.ticket_id, etdr.ticket_id));
                    }
                }
            };
            etdr.price_ticket_type = price_ticket_type_for_etdr_from_rating_details(
                rating_details.iter().map(|r| r.price_ticket_type),
                etdr.price_ticket_type,
            );

            let subscriber_info = {
                let pool = RATING_DB.clone();
                let eq = etag_query.clone();
                tokio::task::spawn_blocking(move || {
                    crate::db::repositories::subscriber_repository::get_subscriber_by_etag(
                        &pool, &eq,
                    )
                })
                .await
                .unwrap_or(Ok(None))
                .unwrap_or(None)
            };

            let mut error_status = 0_i32;
            if !bect_skip_account_check() {
                if let Some(ref sub) = subscriber_info {
                    if sub.account_id <= 0 {
                        error_status = bect::ACCOUNT_IS_NOT_ACTIVE;
                    }
                } else {
                    error_status = bect::SUBSCRIBER_NOT_FOUND;
                }
            }

            let account_id_charge = subscriber_info.as_ref().map(|s| s.account_id).unwrap_or(0);
            etdr.update_from_checkout_local_boo(req, trans_amount);
            save_etdr(etdr.clone());

            let transport_service = TransportTransactionStageService::default();
            let transport_trans_id = etdr.ticket_id;
            let now = crate::utils::now_utc_db_string();
            let checkout_plate_for_db = if req.plate.trim().is_empty() {
                PLATE_EMPTY_SENTINEL
            } else {
                req.plate.trim()
            };
            let charge_status = if error_status != 0 { "FALL" } else { "PENDING" };
            let checkout_plate_status = if error_status == 0 { "1" } else { "0" };
            let checkout_pass = if error_status == 0 { "P" } else { "F" };
            let checkout_pass_reason_id = if error_status != 0 {
                Some(format!("RT-{}", error_status))
            } else {
                None
            };
            let status_str = if etdr.status == 0 { "1" } else { "0" }.to_string();

            let account_trans_id_opt = if account_id_charge > 0 && trans_amount > 0 {
                let (old_bal, old_avail) = {
                    let pool_fetch = RATING_DB.clone();
                    let res = tokio::task::spawn_blocking(move || {
                        get_account_for_charge(&pool_fetch, account_id_charge)
                    })
                    .await
                    .unwrap_or(Ok(None));
                    match res {
                        Ok(Some(a)) => (a.balance, a.available_balance),
                        _ => (0.0, 0.0),
                    }
                };
                let insuff = (trans_amount as f64 - old_avail).max(0.0);
                let new_avail = (old_avail - trans_amount as f64).max(0.0);
                let params = InsertAccountTransactionParams {
                    account_id: account_id_charge,
                    old_balance: old_bal,
                    new_balance: old_bal - trans_amount as f64,
                    amount: trans_amount as f64,
                    description: format!("ETC checkout transport_trans_id={}", transport_trans_id),
                    trans_datetime: now.clone(),
                    old_available_bal: old_avail,
                    new_available_bal: new_avail,
                    insuff_amount: insuff,
                    trans_type: Some("ETC_CHECKOUT".to_string()),
                    transport_trans_id,
                };
                let pool_insert = RATING_DB.clone();
                match tokio::task::spawn_blocking(move || {
                    insert_account_transaction_for_bect_checkout(&pool_insert, &params)
                })
                .await
                {
                    Ok(Ok(id)) => Some(id),
                    _ => None,
                }
            } else {
                None
            };

            let rating_details_for_db = rating_details.clone();
            let _ = transport_service
                .update_checkout_info(
                    transport_trans_id,
                    req.station_out as i64,
                    req.lane_out as i64,
                    checkout_plate_for_db,
                    trans_amount as i64,
                    &now,
                    &now,
                    charge_status,
                    &ticket_type_i32_to_str(etdr.ticket_type),
                    &etdr.vehicle_type,
                    etdr.toll_turning_time_code.as_deref(),
                    Some(&etdr.t_id),
                    "E",
                    etdr.account_id as i64,
                    etdr.charge_in_status.as_deref(),
                    Some("1"),
                    account_trans_id_opt,
                    Some(checkout_plate_status),
                    Some(req.lane_out as i64),
                    Some(checkout_pass),
                    checkout_pass_reason_id.as_deref(),
                    Some(0),
                    Some(&status_str),
                )
                .await;

            let checkin_datetime_str =
                match crate::utils::epoch_to_datetime_utc(etdr.checkin_datetime) {
                    Some(dt) => Some(crate::utils::format_datetime_utc_db(&dt)),
                    None => Some(crate::utils::now_utc_db_string()),
                };

            if !rating_details_for_db.is_empty() {
                let tcd_records: Vec<TransportTransStageTcd> = rating_details_for_db
                    .iter()
                    .map(|rating_detail| {
                        let ticket_type_str = normalize_etag(&rating_detail.ticket_type);
                        let tcd_ticket_type = if ticket_type_str.is_empty() {
                            Some(ticket_type_i32_to_str(etdr.ticket_type))
                        } else {
                            Some(ticket_type_str)
                        };
                        TransportTransStageTcd {
                            transport_stage_tcd_id: 0,
                            transport_trans_id,
                            boo: rating_detail.boo as i64,
                            bot_id: Some(rating_detail.bot_id.unwrap_or(0)),
                            toll_a_id: Some(rating_detail.toll_a_id as i64),
                            toll_b_id: Some(rating_detail.toll_b_id as i64),
                            stage_id: rating_detail.stage_id,
                            price_id: rating_detail.price_id,
                            ticket_type: tcd_ticket_type,
                            price_ticket_type: get_price_ticket_type_from_rating_detail(
                                rating_detail.price_ticket_type,
                            ),
                            price_amount: Some(rating_detail.price_amount as i64),
                            subscription_id: if rating_detail.subscription_id.trim().is_empty() {
                                None
                            } else {
                                Some(rating_detail.subscription_id.clone())
                            },
                            vehicle_type: Some(rating_detail.vehicle_type as i64),
                            mdh_id: None,
                            checkin_datetime: checkin_datetime_str.clone(),
                        }
                    })
                    .collect();
                if save_bect_tcd_list_with_lock(transport_trans_id, &tcd_records)
                    .await
                    .is_ok()
                {
                    let boo_details = vdtc_rating_details_to_boo(&rating_details_for_db);
                    set_tcd_rating_cache(cache.as_ref(), transport_trans_id, true, &boo_details)
                        .await;
                } else {
                    set_pending_tcd_bect(transport_trans_id, &tcd_records).await;
                }
            } else {
                let tcd_record = TransportTransStageTcd {
                    transport_stage_tcd_id: 0,
                    transport_trans_id,
                    boo: 3,
                    bot_id: None,
                    toll_a_id: Some(etdr.station_id as i64),
                    toll_b_id: Some(req.station_out as i64),
                    stage_id: None,
                    price_id: None,
                    ticket_type: Some(ticket_type_i32_to_str(etdr.ticket_type)),
                    price_ticket_type: get_price_ticket_type_from_rating_detail(
                        etdr.price_ticket_type,
                    ),
                    price_amount: Some(trans_amount as i64),
                    subscription_id: None,
                    vehicle_type: Some(etdr.vehicle_type.parse::<i64>().unwrap_or(1)),
                    mdh_id: None,
                    checkin_datetime: checkin_datetime_str.clone(),
                };
                if save_bect_tcd_list_with_lock(
                    transport_trans_id,
                    std::slice::from_ref(&tcd_record),
                )
                .await
                .is_err()
                {
                    set_pending_tcd_bect(transport_trans_id, std::slice::from_ref(&tcd_record))
                        .await;
                }
            }

            if error_status != 0 {
                return Ok((error_status, etdr.ticket_id, etdr.ticket_id));
            }
            Ok((0, etdr.ticket_id, etdr.ticket_id))
        }
        None => Ok((fe::NOT_FOUND_ROUTE_TRANSACTION, 0, 0)),
    }
}
