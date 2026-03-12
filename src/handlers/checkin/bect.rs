//! Handler CHECKIN cho BECT (ETC local): tính phí, lưu DB, trả FE_CHECKIN_IN_RESP.
//! Có kiểm tra tài khoản (tồn tại/active); không kiểm tra số dư. Bật/tắt bằng env BECT_SKIP_ACCOUNT_CHECK.

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::data::subscription_history_cache::get_subscription_history_and_stage_ids_for_etag;
use crate::cache::data::toll_fee_list_cache::get_toll_fee_list_context;
use crate::configs::config::Config;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::configs::rating_db::RATING_DB;
use crate::constants::{bect, fe, lane_type, checkin};
use crate::db::repositories::account_repository::get_account_for_charge;
use crate::db::repositories::subscriber_repository::{get_subscriber_by_etag, SubscriberInfo};
use crate::db::repositories::{
    insert_account_transaction_for_bect_checkout, InsertAccountTransactionParams,
};
use crate::db::repositories::{TransportTransStageTcd, TransportTransactionStage};
use crate::fe_protocol;
use crate::models::TCOCmessages::{FE_CHECKIN, FE_CHECKIN_IN_RESP};
use crate::models::TollCache::{TOLL, TOLL_LANE};
use crate::models::ETDR::{
    get_latest_checkin_by_etag, save_bect_tcd_list_with_lock, save_etdr, set_pending_tcd_bect, ETDR,
};
use crate::services::service::Service;
use crate::services::toll_fee::TollFeeService;
use crate::services::transport_transaction_stage_service::TransportTransactionStageService;
use aes;
use cbc;
use r2d2::Pool;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

use super::common::{
    get_existing_pending_ticket_id_for_ticket_in_id_bect, get_price_ticket_type_from_rating_detail,
    normalize_station_type, price_ticket_type_for_etdr_from_rating_details,
    serialize_and_encrypt_response, ticket_type_from_query_only_for_checkin_resp,
    PLATE_EMPTY_SENTINEL,
};
use crate::handlers::commit::common::{set_tcd_rating_cache, vdtc_rating_details_to_boo};
use crate::utils::{bect_skip_account_check, normalize_etag};

/// Xử lý CHECKIN cho thẻ BECT (ETC) - logic mặc định
pub(crate) async fn handle_bect_checkin(
    fe_checkin: &FE_CHECKIN,
    conn_id: i32,
    toll: &TOLL,
    toll_lane: &TOLL_LANE,
    encryptor: &cbc::Encryptor<aes::Aes128>,
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
) -> Result<(Vec<u8>, i32), Box<dyn Error>> {
    let lane_type = &toll_lane.lane_type;
    tracing::debug!(conn_id, request_id = fe_checkin.request_id, lane_type = %lane_type, etag = %fe_checkin.etag.trim(), "[BECT] CHECKIN handler entered");
    let step_sub = Instant::now();
    let pool_clone = RATING_DB.clone();
    // Chuẩn hóa etag (bỏ null cuối + trim) để khớp khi lưu DB và tra cache, tránh NOT_FOUND
    let etag_query = crate::utils::normalize_etag(&fe_checkin.etag);
    let etag_query_clone = etag_query.clone();

    let subscriber_info =
        tokio::task::spawn_blocking(move || get_subscriber_by_etag(&pool_clone, &etag_query_clone))
            .await
            .unwrap_or(Ok(None))
            .unwrap_or(None);
    let elapsed_ms = step_sub.elapsed().as_millis() as u64;
    tracing::debug!(
        conn_id,
        request_id = fe_checkin.request_id,
        elapsed_ms,
        step = "get_subscriber_by_etag",
        "[BECT] CHECKIN external call finished"
    );

    if let Some(ref _sub) = subscriber_info {
        tracing::debug!(
            request_id = fe_checkin.request_id,
            "[BECT] subscriber found"
        );
    } else {
        tracing::error!(conn_id, request_id = fe_checkin.request_id, etag = %etag_query, "[BECT] CHECKIN subscriber info not found");
    }

    let mut fe_resp: FE_CHECKIN_IN_RESP = FE_CHECKIN_IN_RESP::default();
    fe_resp.message_length = fe_protocol::response_checkin_in_resp_len();
    fe_resp.command_id = fe::CHECKIN_RESP;
    fe_resp.request_id = fe_checkin.request_id;
    fe_resp.session_id = conn_id as i64;
    fe_resp.etag = fe_checkin.etag.clone();
    fe_resp.station = fe_checkin.station;
    fe_resp.lane = fe_checkin.lane;
    // BKS đồng bộ Java: không có subscriber thì trả PLATE_EMPTY_SENTINEL (TCOCMessageChannel.java)
    fe_resp.plate = subscriber_info
        .as_ref()
        .map(|s| s.plate.clone())
        .unwrap_or_else(|| PLATE_EMPTY_SENTINEL.to_string());

    if lane_type == lane_type::OUT {
        tracing::debug!(request_id = fe_checkin.request_id, etag = %fe_checkin.etag, "[BECT] OUT lane lookup ETDR");
        let step_resolve = Instant::now();
        let cache_opt = get_latest_checkin_by_etag(&etag_query).await;
        let db_opt = super::common::get_best_pending_from_sync_or_main_bect(&etag_query).await;
        let etdr_opt = super::common::merge_latest_checkin_etdr(cache_opt, db_opt);
        let elapsed_ms = step_resolve.elapsed().as_millis() as u64;
        tracing::debug!(conn_id, request_id = fe_checkin.request_id, etag = %etag_query, elapsed_ms, step = "resolve_etdr_out", "[BECT] CHECKIN step finished");

        match etdr_opt {
            Some(mut etdr) => {
                if etdr.toll_out != 0
                    || etdr.checkout_datetime != 0
                    || etdr.checkin_commit_datetime == 0
                    || etdr.checkout_commit_datetime != 0
                    || etdr.checkin_datetime == 0
                {
                    tracing::warn!(
                        request_id = fe_checkin.request_id,
                        ref_trans_id = etdr.ref_trans_id,
                        toll_out = etdr.toll_out,
                        "[BECT] ALREADY_CHECKED_OUT"
                    );
                    fe_resp.status = fe::NOT_FOUND_ROUTE_TRANSACTION; // Đã checkout / không tìm thấy giao dịch để checkout
                    fe_resp.ticket_id = etdr.ticket_id;
                    fe_resp.ticket_type = etdr.ticket_type;
                    // Checkout: station/lane = entry (entry lane)
                    fe_resp.station = etdr.station_id;
                    fe_resp.lane = etdr.lane_id;
                    // BKS checkout: ưu tiên etdr.plate → subscriber.plate → fe_checkin.plate (FE gửi khi OUT) → sentinel
                    let (mut plate, mut plate_source) = if !etdr.plate.trim().is_empty() {
                        (etdr.plate.clone(), "etdr")
                    } else if let Some(ref s) = subscriber_info {
                        let p = s.plate.trim();
                        if p.is_empty() {
                            (fe_checkin.plate.trim().to_string(), "fe_checkin")
                        } else {
                            (p.to_string(), "subscriber")
                        }
                    } else {
                        (fe_checkin.plate.trim().to_string(), "fe_checkin")
                    };
                    if plate.is_empty() {
                        plate = PLATE_EMPTY_SENTINEL.to_string();
                        plate_source = "sentinel";
                    }
                    fe_resp.plate = plate;
                    tracing::debug!(request_id = fe_checkin.request_id, plate_source = plate_source, plate = %fe_resp.plate, "[BECT] checkout resp plate (already checked out)");
                    let reply_bytes =
                        serialize_and_encrypt_response(&fe_resp, encryptor).await?;
                    return Ok((reply_bytes, fe_resp.status));
                }

                tracing::debug!(
                    request_id = fe_checkin.request_id,
                    ref_trans_id = etdr.ref_trans_id,
                    "[BECT] ETDR found"
                );

                tracing::debug!(request_id = fe_checkin.request_id, etdr = ?etdr, "[BECT] ETDR");

                let subscriber_id = subscriber_info.as_ref().map(|s| s.subscriber_id);
                let etag_str = crate::utils::normalize_etag(&fe_checkin.etag);
                let step_fee_ctx = Instant::now();
                let (list_ctx, (subscription_history, subscription_stage_set)) = tokio::join!(
                    get_toll_fee_list_context(db_pool.clone(), cache.clone(), &etag_str),
                    get_subscription_history_and_stage_ids_for_etag(cache.clone(), &etag_str),
                );
                let elapsed_ms = step_fee_ctx.elapsed().as_millis() as u64;
                tracing::debug!(conn_id, request_id = fe_checkin.request_id, etag = %etag_str, elapsed_ms, step = "get_toll_fee_list_context_and_subscription", "[BECT] CHECKIN external call finished");
                // Get fee calculation time: prefer checkout_datetime from ETDR, else use current time
                let trans_datetime = if etdr.checkout_datetime > 0 {
                    crate::utils::epoch_to_datetime_utc(etdr.checkout_datetime)
                        .map(|dt| dt.naive_utc())
                } else {
                    Some(chrono::Utc::now().naive_utc())
                };
                let (trans_amount, rating_details) = {
                    let step_calc = Instant::now();
                    let out = match TollFeeService::calculate_toll_fee(
                        db_pool.clone(),
                        cache.clone(),
                        etdr.station_id,
                        fe_checkin.station,
                        etdr.vehicle_type.parse::<i32>().unwrap_or(1),
                        &super::common::ticket_type_i32_to_str(etdr.ticket_type),
                        subscriber_id,
                        list_ctx.as_ref(),
                        None,
                        Some(&subscription_stage_set),
                        subscription_history.as_deref(),
                        trans_datetime,
                    )
                    .await
                    {
                        Ok(t) => {
                            let elapsed_ms = step_calc.elapsed().as_millis() as u64;
                            tracing::debug!(
                                conn_id,
                                request_id = fe_checkin.request_id,
                                elapsed_ms,
                                step = "calculate_toll_fee",
                                "[BECT] CHECKIN external call finished"
                            );
                            t
                        }
                        Err(e) => {
                            let elapsed_ms = step_calc.elapsed().as_millis() as u64;
                            tracing::debug!(conn_id, request_id = fe_checkin.request_id, elapsed_ms, step = "calculate_toll_fee", error = %e, "[BECT] CHECKIN external call finished");
                            if crate::configs::config::Config::get()
                                .allow_zero_fee_when_no_price_or_route
                            {
                                tracing::warn!(
                                    request_id = fe_checkin.request_id,
                                    etag = %fe_checkin.etag,
                                    station_in = etdr.station_id,
                                    station_out = fe_checkin.station,
                                    error = %e,
                                    "[BECT] toll fee bypass: using 0 when no price/route"
                                );
                                (0, vec![])
                            } else {
                                tracing::warn!(
                                    request_id = fe_checkin.request_id,
                                    etag = %fe_checkin.etag,
                                    station_in = etdr.station_id,
                                    station_out = fe_checkin.station,
                                    error = %e,
                                    "[BECT] toll fee error: no price config or cannot calculate"
                                );
                                tracing::error!(
                                    provider = "BECT",
                                    conn_id,
                                    request_id = fe_checkin.request_id,
                                    etag = %fe_checkin.etag.trim(),
                                    status = bect::NOT_FOUND_PRICE_INFO,
                                    error_detail = crate::utils::fe_status_detail(bect::NOT_FOUND_PRICE_INFO),
                                    flow = "CHECKIN",
                                    detail = "no price config or cannot calculate toll fee",
                                    "[BECT] CHECKIN failed"
                                );
                                let mut fe_resp = FE_CHECKIN_IN_RESP::default();
                                fe_resp.message_length =
                                    fe_protocol::response_checkin_in_resp_len();
                                fe_resp.command_id = fe::CHECKIN_RESP;
                                fe_resp.request_id = fe_checkin.request_id;
                                fe_resp.session_id = conn_id as i64;
                                fe_resp.status = bect::NOT_FOUND_PRICE_INFO;
                                fe_resp.etag = fe_checkin.etag.clone();
                                fe_resp.station = fe_checkin.station;
                                fe_resp.lane = fe_checkin.lane;
                                let reply_bytes = serialize_and_encrypt_response(
                                    &fe_resp,
                                    encryptor,
                                )
                                .await?;
                                return Ok((reply_bytes, fe_resp.status));
                            }
                        }
                    };
                    out
                };

                let rating_details_for_db = rating_details.clone();
                etdr.price_ticket_type = price_ticket_type_for_etdr_from_rating_details(
                    rating_details.iter().map(|r| r.price_ticket_type),
                    etdr.price_ticket_type,
                );

                tracing::debug!(
                    request_id = fe_checkin.request_id,
                    trans_amount,
                    segments = rating_details.len(),
                    "[BECT] toll fee"
                );

                // Kiểm tra tài khoản (subscriber/account tồn tại, active); không kiểm tra số dư; có thể bypass bằng env.
                // Không return ngay khi lỗi: lưu dữ liệu giao dịch trước, rồi mới trả về lỗi.
                let mut error_status = 0_i32;
                if !bect_skip_account_check() {
                    if let Some(ref sub) = subscriber_info {
                        let account_id = sub.account_id;
                        if account_id <= 0 {
                            tracing::warn!(conn_id, request_id = fe_checkin.request_id, etag = %etag_query, account_id, "[BECT] CHECKOUT account check: account not found or inactive");
                            error_status = bect::ACCOUNT_IS_NOT_ACTIVE;
                        }
                    } else {
                        tracing::warn!(conn_id, request_id = fe_checkin.request_id, etag = %etag_query, "[BECT] CHECKOUT account check: subscriber not found");
                        error_status = bect::SUBSCRIBER_NOT_FOUND;
                    }
                }

                let account_id_charge = subscriber_info.as_ref().map(|s| s.account_id).unwrap_or(0);
                let account_for_trans: Option<crate::db::repositories::AccountForCharge> = None;

                // Luôn lưu dữ liệu giao dịch (ETDR + TRANSPORT_TRANSACTION_STAGE + TCD) trước khi trả về (dù thành công hay lỗi).
                etdr.update_from_checkout_local(fe_checkin, trans_amount);
                save_etdr(etdr.clone());
                tracing::debug!(
                    request_id = fe_checkin.request_id,
                    toll_out = etdr.toll_out,
                    "[BECT] ETDR checkout cached"
                );

                let transport_service = TransportTransactionStageService::default();
                let transport_trans_id = etdr.ticket_id;
                let now = crate::utils::now_utc_db_string();

                let checkout_plate = if fe_checkin.plate.is_empty() {
                    etdr.plate.clone()
                } else {
                    fe_checkin.plate.clone()
                };
                // Giá trị ghi DB phải trùng với fe_resp: nếu rỗng thì lưu PLATE_EMPTY_SENTINEL
                let checkout_plate_for_db = if checkout_plate.trim().is_empty() {
                    PLATE_EMPTY_SENTINEL
                } else {
                    checkout_plate.trim()
                };

                let charge_status = if error_status != 0 { "FALL" } else { "PENDING" };

                // Ghi nhận trạng thái giao dịch vào ACCOUNT_TRANSACTION (BECT checkout): tạo bản ghi chưa commit (AUTOCOMMIT=2).
                let account_trans_id_opt = if account_id_charge > 0 && trans_amount > 0 {
                    let (old_bal, old_avail) = match &account_for_trans {
                        Some(a) => (a.balance, a.available_balance),
                        None => {
                            let pool_fetch = RATING_DB.clone();
                            let id_fetch = account_id_charge;
                            let res = tokio::task::spawn_blocking(move || {
                                get_account_for_charge(&pool_fetch, id_fetch)
                            })
                            .await
                            .unwrap_or(Ok(None));
                            match res {
                                Ok(Some(a)) => (a.balance, a.available_balance),
                                _ => (0.0, 0.0),
                            }
                        }
                    };
                    let insuff = (trans_amount as f64 - old_avail).max(0.0);
                    let new_avail = (old_avail - trans_amount as f64).max(0.0);
                    let params = InsertAccountTransactionParams {
                        account_id: account_id_charge,
                        old_balance: old_bal,
                        new_balance: old_bal - trans_amount as f64,
                        amount: trans_amount as f64,
                        description: format!(
                            "ETC checkout transport_trans_id={}",
                            transport_trans_id
                        ),
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
                        Ok(Ok(id)) => {
                            tracing::debug!(
                                request_id = fe_checkin.request_id,
                                account_transaction_id = id,
                                transport_trans_id,
                                "[BECT] ACCOUNT_TRANSACTION created"
                            );
                            Some(id)
                        }
                        Ok(Err(e)) => {
                            tracing::error!(conn_id, request_id = fe_checkin.request_id, transport_trans_id, error = %e, "[BECT] CHECKOUT ACCOUNT_TRANSACTION insert failed");
                            None
                        }
                        Err(_) => None,
                    }
                } else {
                    None
                };

                // checkout_status per Java CheckioStatus: "1" = CHECKIN (đã checkout, chờ commit hoặc lỗi), "2" = PASS (đã commit success)
                let checkout_plate_status = if error_status == 0 { "1" } else { "0" };
                let checkout_pass = if error_status == 0 { "P" } else { "F" };
                let checkout_pass_reason_id = if error_status != 0 {
                    Some(format!("RT-{}", error_status))
                } else {
                    None
                };
                let status_str = if etdr.status == 0 { "1" } else { "0" }.to_string(); // DB: 1 = success, 0 = error
                match transport_service
                    .update_checkout_info(
                        transport_trans_id,
                        fe_checkin.station as i64,
                        fe_checkin.lane as i64,
                        checkout_plate_for_db,
                        trans_amount as i64,
                        &now,
                        &now,
                        charge_status,
                        &super::common::ticket_type_i32_to_str(etdr.ticket_type),
                        &etdr.vehicle_type,
                        etdr.toll_turning_time_code.as_deref(),
                        Some(&etdr.t_id),
                        "E",
                        etdr.account_id as i64,
                        etdr.charge_in_status.as_deref(),
                        Some("1"), // CHECKIN: đã checkout (PENDING hoặc FALL), chờ commit
                        account_trans_id_opt,
                        Some(checkout_plate_status),
                        Some(fe_checkin.lane as i64),
                        Some(checkout_pass),
                        checkout_pass_reason_id.as_deref(),
                        Some(0), // CHECKOUT_IMG_COUNT: 0 tại checkout, commit sẽ ghi đè
                        Some(&status_str), // Thống nhất với BOO: STATUS = etdr.status tại checkout
                    )
                    .await
                {
                    Ok(true) => {
                        tracing::debug!(
                            request_id = fe_checkin.request_id,
                            "[BECT] Stage checkout updated in DB"
                        );

                        // Luôn lưu ratingDetail vào TRANSPORT_TRANS_STAGE_TCD (load existing → update/insert theo toll_a,toll_b → xóa bản ghi thừa).
                        let checkin_datetime_str =
                            match crate::utils::epoch_to_datetime_utc(etdr.checkin_datetime) {
                                Some(dt) => Some(crate::utils::format_datetime_utc_db(&dt)),
                                None => {
                                    tracing::warn!(
                                        request_id = fe_checkin.request_id,
                                        checkin_datetime = etdr.checkin_datetime,
                                        "[BECT] Invalid checkin_datetime, using current time"
                                    );
                                    Some(crate::utils::now_utc_db_string())
                                }
                            };

                        if !rating_details_for_db.is_empty() {
                            let tcd_records: Vec<TransportTransStageTcd> = rating_details_for_db
                                .iter()
                                .map(|rating_detail| {
                                    let ticket_type_str =
                                        normalize_etag(&rating_detail.ticket_type);
                                    let tcd_ticket_type = if ticket_type_str.is_empty() {
                                        Some(super::common::ticket_type_i32_to_str(
                                            etdr.ticket_type,
                                        ))
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
                                        subscription_id: if rating_detail
                                            .subscription_id
                                            .trim()
                                            .is_empty()
                                        {
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

                            match save_bect_tcd_list_with_lock(transport_trans_id, &tcd_records)
                                .await
                            {
                                Ok(()) => {
                                    tracing::debug!(
                                        request_id = fe_checkin.request_id,
                                        transport_trans_id,
                                        count = tcd_records.len(),
                                        "[BECT] TRANSPORT_TRANS_STAGE_TCD saved"
                                    );
                                    let boo_details =
                                        vdtc_rating_details_to_boo(&rating_details_for_db);
                                    set_tcd_rating_cache(
                                        cache.as_ref(),
                                        transport_trans_id,
                                        true,
                                        &boo_details,
                                    )
                                    .await;
                                }
                                Err(e) => {
                                    tracing::error!(
                                        conn_id,
                                        request_id = fe_checkin.request_id,
                                        transport_trans_id,
                                        error = %e,
                                        "[BECT] TRANSPORT_TRANS_STAGE_TCD save_tcd_list failed"
                                    );
                                    set_pending_tcd_bect(transport_trans_id, &tcd_records).await;
                                }
                            }
                        } else {
                            // rating_details rỗng (vd. không có route hoặc giá hết hiệu lực): lưu một bản ghi TCD tổng hợp để commit có thể đọc rating_detail từ TCD
                            let tcd_record = TransportTransStageTcd {
                                transport_stage_tcd_id: 0,
                                transport_trans_id,
                                boo: 3, // BECT
                                bot_id: None,
                                toll_a_id: Some(etdr.station_id as i64),
                                toll_b_id: Some(fe_checkin.station as i64),
                                stage_id: None,
                                price_id: None,
                                ticket_type: Some(super::common::ticket_type_i32_to_str(
                                    etdr.ticket_type,
                                )),
                                price_ticket_type: get_price_ticket_type_from_rating_detail(
                                    etdr.price_ticket_type,
                                ),
                                price_amount: Some(trans_amount as i64),
                                subscription_id: None,
                                vehicle_type: Some(etdr.vehicle_type.parse::<i64>().unwrap_or(1)),
                                mdh_id: None,
                                checkin_datetime: checkin_datetime_str.clone(),
                            };
                            match save_bect_tcd_list_with_lock(
                                transport_trans_id,
                                std::slice::from_ref(&tcd_record),
                            )
                            .await
                            {
                                Ok(()) => {
                                    tracing::debug!(request_id = fe_checkin.request_id, transport_trans_id, "[BECT] TRANSPORT_TRANS_STAGE_TCD saved (single summary row)");
                                }
                                Err(e) => {
                                    tracing::error!(conn_id, request_id = fe_checkin.request_id, transport_trans_id, error = %e, "[BECT] CHECKOUT save TRANSPORT_TRANS_STAGE_TCD summary failed");
                                    set_pending_tcd_bect(
                                        transport_trans_id,
                                        std::slice::from_ref(&tcd_record),
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                    Ok(false) => {
                        tracing::warn!(
                            conn_id,
                            request_id = fe_checkin.request_id,
                            transport_trans_id,
                            etag = %fe_checkin.etag.trim(),
                            "[BECT] CHECKOUT TRANSPORT_TRANSACTION_STAGE update returned false"
                        );
                        // Đảm bảo ETDR vẫn có dữ liệu mới nhất khi ghi DB lỗi
                        save_etdr(etdr.clone());
                    }
                    Err(e) => {
                        tracing::error!(conn_id, request_id = fe_checkin.request_id, transport_trans_id, etag = %fe_checkin.etag.trim(), error = %e, "[BECT] CHECKOUT TRANSPORT_TRANSACTION_STAGE update failed");
                        save_etdr(etdr.clone());
                    }
                }

                // Sau khi đã lưu dữ liệu giao dịch: nếu có lỗi kiểm tra tài khoản (subscriber/account) thì trả về lỗi.
                if error_status != 0 {
                    tracing::error!(
                        provider = "BECT",
                        conn_id,
                        request_id = fe_checkin.request_id,
                        etag = %etag_query,
                        transport_trans_id = etdr.ticket_id,
                        status = error_status,
                        error_detail = crate::utils::fe_status_detail(error_status),
                        flow = "CHECKOUT",
                        detail = "account check failed (subscriber/account/balance)",
                        "[BECT] CHECKOUT failed"
                    );
                    fe_resp.status = error_status;
                    fe_resp.ticket_id = etdr.ticket_id;
                    // BKS checkout: ưu tiên etdr.plate → subscriber.plate → fe_checkin.plate → sentinel
                    let (mut plate, mut plate_source) = if !etdr.plate.trim().is_empty() {
                        (etdr.plate.clone(), "etdr")
                    } else if let Some(ref s) = subscriber_info {
                        let p = s.plate.trim();
                        if p.is_empty() {
                            (fe_checkin.plate.trim().to_string(), "fe_checkin")
                        } else {
                            (p.to_string(), "subscriber")
                        }
                    } else {
                        (fe_checkin.plate.trim().to_string(), "fe_checkin")
                    };
                    if plate.is_empty() {
                        plate = PLATE_EMPTY_SENTINEL.to_string();
                        plate_source = "sentinel";
                    }
                    fe_resp.plate = plate;
                    // Checkout: station/lane = entry (entry lane)
                    fe_resp.station = etdr.station_id;
                    fe_resp.lane = etdr.lane_id;
                    tracing::debug!(request_id = fe_checkin.request_id, plate_source = plate_source, plate = %fe_resp.plate, "[BECT] checkout resp plate (error path)");
                    let reply_bytes =
                        serialize_and_encrypt_response(&fe_resp, encryptor).await?;
                    return Ok((reply_bytes, fe_resp.status));
                }

                fe_resp.status = 0;
                fe_resp.ticket_id = etdr.ticket_id;
                fe_resp.vehicle_type = etdr.vehicle_type.parse::<i32>().unwrap_or(1);
                // BKS checkout: ưu tiên etdr.plate → subscriber.plate → fe_checkin.plate → sentinel
                let (mut plate, mut plate_source) = if !etdr.plate.trim().is_empty() {
                    (etdr.plate.clone(), "etdr")
                } else if let Some(ref s) = subscriber_info {
                    let p = s.plate.trim();
                    if p.is_empty() {
                        (fe_checkin.plate.trim().to_string(), "fe_checkin")
                    } else {
                        (p.to_string(), "subscriber")
                    }
                } else {
                    (fe_checkin.plate.trim().to_string(), "fe_checkin")
                };
                if plate.is_empty() {
                    plate = PLATE_EMPTY_SENTINEL.to_string();
                    plate_source = "sentinel";
                }
                fe_resp.plate = plate;
                tracing::debug!(request_id = fe_checkin.request_id, plate_source = plate_source, plate = %fe_resp.plate, "[BECT] checkout resp plate (success)");
                // Checkout: station/lane = entry (entry lane)
                fe_resp.station = etdr.station_id;
                fe_resp.lane = etdr.lane_id;
                fe_resp.ticket_type = etdr.ticket_type;
                fe_resp.price = trans_amount;
                fe_resp.plate_type = 0;
                fe_resp.price_ticket_type = etdr.price_ticket_type;
            }
            None => {
                tracing::error!(
                    provider = "BECT",
                    conn_id,
                    request_id = fe_checkin.request_id,
                    etag = %etag_query,
                    status = fe::NOT_FOUND_ROUTE_TRANSACTION,
                    error_detail = crate::utils::fe_status_detail(fe::NOT_FOUND_ROUTE_TRANSACTION),
                    detail = "CHECKOUT no checkIn record",
                    "[BECT] CHECKOUT failed"
                );
                fe_resp.status = fe::NOT_FOUND_ROUTE_TRANSACTION;
                fe_resp.ticket_id = 0;
            }
        }
    } else {
        // Kiểm tra số dư tối thiểu khi check-in lane IN. Java: chỉ khi stationLane ∈ {OPEN=0, HOLD=6, REFUND=7, ONLY_REFUND=8, CAM=9} mới gọi doCheckIn (hold+min). Rust: áp dụng cho lane IN; nếu bect_minimum_balance_checkin_open_only thì chỉ khi trạm mở (toll_type O). Xem docs/BECT_LANETYPE_MIN_BALANCE.md.
        let account_id = subscriber_info.as_ref().map(|s| s.account_id).unwrap_or(0);
        let open_only = Config::get().bect_minimum_balance_checkin_open_only;
        let is_open_station = super::common::normalize_station_type(&toll.toll_type) == "O";
        let apply_min_balance_check = !open_only || is_open_station;
        if account_id > 0 && !bect_skip_account_check() && apply_min_balance_check {
            let min_bal = Config::get().bect_minimum_balance_checkin;
            if min_bal > 0 {
                let pool_min = RATING_DB.clone();
                let id_fetch = account_id;
                let account_opt = tokio::task::spawn_blocking(move || {
                    get_account_for_charge(&pool_min, id_fetch)
                })
                .await
                .unwrap_or(Ok(None))
                .unwrap_or(None);
                if let Some(acc) = account_opt {
                    if acc.available_balance < min_bal as f64 {
                        fe_resp.status = bect::ACCOUNT_NOT_ENOUGH_MONEY;
                        tracing::error!(
                            provider = "BECT",
                            conn_id,
                            request_id = fe_checkin.request_id,
                            etag = %fe_checkin.etag.trim(),
                            account_id,
                            available_balance = acc.available_balance,
                            minimum_balance = min_bal,
                            status = bect::ACCOUNT_NOT_ENOUGH_MONEY,
                            error_detail = crate::utils::fe_status_detail(bect::ACCOUNT_NOT_ENOUGH_MONEY),
                            flow = "CHECKIN",
                            detail = "insufficient balance (min balance check)",
                            "[BECT] CHECKIN failed"
                        );
                        let reply_bytes =
                            serialize_and_encrypt_response(&fe_resp, encryptor)
                                .await?;
                        return Ok((reply_bytes, fe_resp.status));
                    }
                }
            }
        }

        tracing::debug!(request_id = fe_checkin.request_id, etag = %fe_checkin.etag, "[BECT] IN lane create ETDR");

        // New tag check-in IN always creates new transaction; idempotent chỉ theo ticket_in_id (retry cùng request).
        let transport_service = TransportTransactionStageService::default();
        let mut ticket_id_from_sequence = transport_service
            .get_ticket_id_for_bect_checkin(fe_checkin.request_id)
            .await;
        if let Some(existing_tid) =
            get_existing_pending_ticket_id_for_ticket_in_id_bect(ticket_id_from_sequence).await
        {
            tracing::debug!(
                conn_id,
                request_id = fe_checkin.request_id,
                ticket_in_id = ticket_id_from_sequence,
                ticket_id = existing_tid,
                "[BECT] CHECKIN IN idempotent: reuse existing ticket_id for ticket_in_id"
            );
            ticket_id_from_sequence = existing_tid;
        }

        let mut etdr = ETDR::from_checkin_bect(fe_checkin);
        etdr.ticket_id = ticket_id_from_sequence;
        let plate = subscriber_info
            .as_ref()
            .map(|s| s.plate.clone())
            .unwrap_or(fe_checkin.plate.clone());
        etdr.plate = if plate.trim().is_empty() {
            checkin::PLATE_EMPTY_SENTINEL.to_string()
        } else {
            plate
        };
        etdr.vehicle_type = subscriber_info
            .as_ref()
            .map(|s| s.vehicle_type.clone())
            .unwrap_or("1".to_string());
        // Override vehicle_type từ EX_LIST (trạm mở) nếu có: TYPE_STATION 0/1, khớp station_id = toll_id hoặc 0. Java: getFirstElement(stationId|vehicleId, getCacheExList()).
        let etag_str = crate::utils::normalize_etag(&fe_checkin.etag);
        if !etag_str.is_empty() {
            if let Some(ref list_ctx) =
                get_toll_fee_list_context(db_pool.clone(), cache.clone(), &etag_str).await
            {
                if let Some(vt) = crate::services::toll_fee::vehicle_type_from_ex_list_open(
                    &list_ctx.ex_list,
                    toll.toll_id as i64,
                ) {
                    if !vt.trim().is_empty() {
                        etdr.vehicle_type = vt;
                    }
                }
            }
        }
        // Dùng cho fe_resp; khi verify được bản ghi DB thì cập nhật từ DB để trả về đúng loại vé và BKS
        let mut etdr_for_resp = etdr.clone();
        // Điền subscriber vào ETDR để retry save DB có đủ thông tin
        if let Some(ref s) = subscriber_info {
            etdr.sub_id = s.subscriber_id as i32;
            etdr.account_id = s.account_id as i32;
            etdr.vehicle_id = s.vehicle_id as i32;
            etdr.etag_key = if s.etag_id != 0 {
                Some(s.etag_id as i32)
            } else {
                None
            };
        }

        let transport_stage = create_transport_transaction_stage_from_bect_checkin(
            fe_checkin,
            &etdr,
            toll,
            toll_lane,
            ticket_id_from_sequence,
            subscriber_info.as_ref(),
        );
        tracing::debug!(
            request_id = fe_checkin.request_id,
            transport_trans_id = transport_stage.transport_trans_id,
            checkin_plate = ?transport_stage.checkin_plate.as_deref(),
            plate = ?transport_stage.plate.as_deref(),
            plate_from_toll = ?transport_stage.plate_from_toll.as_deref(),
            "[BECT] Stage built, saving to DB"
        );
        let transport_service = TransportTransactionStageService::default();
        match transport_service.save(&transport_stage).await {
            Ok(transport_trans_id) => {
                tracing::debug!(
                    request_id = fe_checkin.request_id,
                    transport_trans_id,
                    "[BECT] Stage saved"
                );

                // save() đã verify trong DB và ghi cache → get_by_id một lần (thường hit cache).
                let verified_record_opt = match transport_service
                    .get_by_id(transport_trans_id)
                    .await
                {
                    Ok(Some(r)) => Some(r),
                    Ok(None) => {
                        tracing::warn!(conn_id, request_id = fe_checkin.request_id, transport_trans_id, etag = %fe_checkin.etag.trim(), "[BECT] CHECKIN TRANSPORT_TRANSACTION_STAGE not found after save (cache/DB miss)");
                        None
                    }
                    Err(e) => {
                        tracing::warn!(conn_id, request_id = fe_checkin.request_id, transport_trans_id, error = %e, "[BECT] CHECKIN get_by_id after save failed");
                        None
                    }
                };

                match verified_record_opt {
                    Some(_db_record) => {
                        // ETDR cache làm chuẩn: cập nhật cache từ etdr đã có (từ checkin), không ghi đè từ DB.
                        etdr.db_saved = true;
                        save_etdr(etdr.clone());
                        etdr_for_resp = etdr.clone();
                        tracing::debug!(request_id = fe_checkin.request_id, transport_trans_id, ticket_id = ticket_id_from_sequence, etag = %fe_checkin.etag, "[BECT] ETDR cache updated (source of truth from checkin, DB verified)");
                    }
                    None => {
                        tracing::warn!(conn_id, request_id = fe_checkin.request_id, transport_trans_id, etag = %fe_checkin.etag.trim(), "[BECT] CHECKIN TRANSPORT_TRANSACTION_STAGE not found after save and verify retries");
                        // Luồng lỗi: ghi lại status = "0" vào DB
                        let mut stage_status_zero = transport_stage.clone();
                        stage_status_zero.status = Some("0".to_string());
                        if let Err(e) = transport_service
                            .update(transport_trans_id, &stage_status_zero)
                            .await
                        {
                            tracing::warn!(conn_id, request_id = fe_checkin.request_id, transport_trans_id, error = %e, "[BECT] CHECKIN update STATUS to 0 after verify fail failed");
                        }
                        etdr.check = fe::VERIFY_DB_FAILED;
                        etdr.db_saved = false;
                        etdr.db_save_retry_count += 1;
                        save_etdr(etdr.clone());
                    }
                }
            }
            Err(e) => {
                tracing::error!(conn_id, request_id = fe_checkin.request_id, etag = %fe_checkin.etag.trim(), error = %e, "[BECT] CHECKIN save TRANSPORT_TRANSACTION_STAGE failed");
                etdr.check = fe::SAVE_DB_ERROR;
                // Vẫn save ETDR vào cache với db_saved=false để có thể retry save DB sau
                etdr.db_saved = false;
                etdr.db_save_retry_count += 1;
                save_etdr(etdr.clone());
            }
        }

        // Đảm bảo fe_resp và DB đồng nhất: status=0 chỉ khi đã verify bản ghi DB (etdr_for_resp từ DB).
        // Khi verify thất bại đã set etdr.check = VERIFY_DB_FAILED ở nhánh None.
        fe_resp.status = etdr.check;
        fe_resp.ticket_id = ticket_id_from_sequence;
        // Tại checkin chưa tính phí nên chưa có loại vé trong DB; trả về mặc định L (giống VDTC/VETC).
        fe_resp.ticket_type = ticket_type_from_query_only_for_checkin_resp("");
        fe_resp.price = etdr_for_resp.price;
        fe_resp.vehicle_type = etdr_for_resp.vehicle_type.parse::<i32>().unwrap_or(1);
        fe_resp.plate = if etdr_for_resp.plate.trim().is_empty() {
            subscriber_info
                .as_ref()
                .map(|s| s.plate.clone())
                .filter(|p| !p.trim().is_empty())
                .unwrap_or_else(|| PLATE_EMPTY_SENTINEL.to_string())
        } else {
            etdr_for_resp.plate.clone()
        };
        fe_resp.plate_type = etdr_for_resp.plate_status;
        fe_resp.price_ticket_type = etdr_for_resp.price_ticket_type;
    }

    tracing::debug!(
        request_id = fe_checkin.request_id,
        status = fe_resp.status,
        "[BECT] resp"
    );
    let step_serialize = Instant::now();
    let reply_bytes = serialize_and_encrypt_response(&fe_resp, encryptor).await?;
    let elapsed_ms = step_serialize.elapsed().as_millis() as u64;
    tracing::debug!(
        conn_id,
        request_id = fe_checkin.request_id,
        elapsed_ms,
        step = "serialize_and_encrypt_response",
        "[BECT] CHECKIN step finished"
    );
    tracing::debug!(
        request_id = fe_checkin.request_id,
        reply_len = reply_bytes.len(),
        "[BECT] reply"
    );

    Ok((reply_bytes, fe_resp.status))
}

/// Tạo TransportTransactionStage từ dữ liệu checkin BECT
pub(crate) fn create_transport_transaction_stage_from_bect_checkin(
    fe_checkin: &FE_CHECKIN,
    etdr: &ETDR,
    toll: &TOLL,
    toll_lane: &TOLL_LANE,
    transport_trans_id: i64,
    subscriber_info: Option<&SubscriberInfo>,
) -> TransportTransactionStage {
    let now = crate::utils::now_utc_db_string();

    let (checkin_pass, checkin_pass_reason_id) = if etdr.check == 0 {
        (Some("P".to_string()), None)
    } else {
        (Some("F".to_string()), Some(format!("RT-{}", etdr.check)))
    };

    // Chuẩn hóa giống BOO: rating_type mặc định "E" (ETC), CHECKIN_PLATE_STATUS "1"/"0" (pass/fail)
    let rating_type = Some(etdr.rating_type.clone().unwrap_or_else(|| "E".to_string()));
    let checkin_plate_status = Some(if etdr.check == 0 {
        "1".to_string()
    } else {
        "0".to_string()
    });

    TransportTransactionStage {
        transport_trans_id,
        subscriber_id: subscriber_info.map(|s| s.subscriber_id),
        etag_id: subscriber_info.map(|s| s.etag_id),
        vehicle_id: subscriber_info.map(|s| s.vehicle_id),
        checkin_toll_id: Some(toll.toll_id as i64),
        checkin_lane_id: Some(toll_lane.lane_code as i64),
        checkin_commit_datetime: None,
        checkin_channel: Some(fe_checkin.lane as i64),
        checkin_pass,
        checkin_pass_reason_id,
        checkout_toll_id: None,
        checkout_lane_id: None,
        checkout_commit_datetime: None,
        checkout_channel: None,
        checkout_pass: None,
        checkout_pass_reason_id: None,
        charge_status: None,
        charge_type: None, // Chỉ gán khi checkout (sau khi tính phí), giống VDTC/VETC
        total_amount: None,
        account_id: subscriber_info.map(|s| s.account_id),
        account_trans_id: None,
        checkin_datetime: Some(now.clone()),
        checkout_datetime: None,
        checkin_status: Some("1".to_string()), // CHECKIN: chờ commit checkin (theo Java CheckioStatus)
        etag_number: Some(normalize_etag(&fe_checkin.etag)),
        request_id: Some(fe_checkin.request_id),
        checkin_plate: Some({
            let p = normalize_etag(&etdr.plate);
            if p.is_empty() {
                PLATE_EMPTY_SENTINEL.to_string()
            } else {
                p
            }
        }),
        checkin_plate_status,
        checkout_plate: None,
        checkout_plate_status: None,
        plate_from_toll: {
            let plate_fe = normalize_etag(&fe_checkin.plate);
            if plate_fe.is_empty() {
                None
            } else {
                Some(plate_fe)
            }
        },
        img_count: Some(etdr.image_count),
        checkin_img_count: Some(etdr.image_count),
        checkout_img_count: None,
        status: Some("1".to_string()), // Thống nhất với BOO: checkin mới = "1"
        last_update: Some(now.clone()),
        notif_scan: None,
        toll_type: Some(normalize_station_type(&toll.toll_type)),
        rating_type,
        insert_datetime: Some(now.clone()),
        insuff_amount: None,
        checkout_status: None,
        bitwise_scan: None,
        plate: Some({
            let p = normalize_etag(&etdr.plate);
            if p.is_empty() {
                PLATE_EMPTY_SENTINEL.to_string()
            } else {
                p
            }
        }), // Luôn gán (checkin); đồng bộ với fe_resp, checkout cập nhật trong update_checkout_info
        vehicle_type: Some(etdr.vehicle_type.clone()),
        fe_vehicle_length: None,
        fe_commit_amount: Some(etdr.commit_amount as i64),
        fe_weight: Some(etdr.weight as i64),
        fe_reason_id: if etdr.reason_id != 0 {
            Some(etdr.reason_id as i64)
        } else {
            Some(0)
        },
        vehicle_type_profile: Some("STD".to_string()),
        boo: Some(3), // BECT = 3 (ETC)
        boo_transport_trans_id: Some(transport_trans_id),
        subscription_ids: None,
        checkin_shift: None,
        checkout_shift: None,
        turning_code: None,
        checkin_tid: Some(normalize_etag(&fe_checkin.tid)),
        checkout_tid: None,
        charge_in: etdr.charge_in.clone(),
        charge_trans_id: None,
        balance: None,
        charge_in_status: None,
        charge_datetime: None,
        charge_in_104: None,
        fe_online_status: None,
        mdh_id: None,
        chd_type: None,
        chd_ref_id: None,
        chd_reason: None,
        fe_trans_id: None,
        transition_close: Some("F".to_string()),
        voucher_code: None,
        voucher_used_amount: None,
        voucher_amount: None,
        transport_sync_id: None,
        ticket_in_id: Some(transport_trans_id),
        hub_id: etdr.hub_id,
        ticket_eTag_id: None,
        ticket_out_id: None,
        token_id: None,
        acs_account_no: None,
        // Thống nhất với BOO: lưu số etag vào BOO_ETAG. Cột DB có giới hạn độ dài nên cắt theo MAX_BOO_ETAG_LEN.
        boo_etag: Some("3".to_string()),
        sub_charge_in: None,
        sync_status: None,
    }
}
