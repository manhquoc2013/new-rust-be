//! Handler COMMIT: cập nhật TRANSPORT_TRANSACTION_STAGE, trả FE COMMIT_RESP (resp).
//! Cặp req/resp từ FE: FE gửi COMMIT (req) → processor gọi handle_commit → handler xử lý, trả COMMIT_RESP (resp) cho FE.
//! Trước khi trả về lỗi luôn lưu dữ liệu giao dịch (ETDR + DB). OUT lane có kiểm tra tài khoản (env bypass).

use super::common::{
    get_rating_detail_cached, invalidate_tcd_rating_cache, serialize_and_encrypt_commit_response,
};
use super::kafka_payload::{
    build_checkin_payload_from_etdr, build_checkout_payload_from_etdr, send_checkin_to_kafka,
    send_checkout_to_kafka, CheckinPayloadOverrides, CheckoutPayloadOverrides,
};
use crate::cache::config::cache_manager::CacheManager;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::configs::rating_db::RATING_DB;
use crate::constants::{bect, commit_retry, fe, lane_type};
use crate::db::repositories::subscriber_repository::get_subscriber_by_etag;
use crate::db::repositories::transport_transaction_stage::{
    TransportTransactionStage, TransportTransactionStageRepository,
};
use crate::db::repositories::{
    allow_commit_or_rollback, get_account_transaction_for_commit,
    update_account_transaction_commit_status, AUTOCOMMIT_STATUS_FAIL, AUTOCOMMIT_STATUS_SUCCESS,
};
use crate::fe_protocol;
use crate::handlers::checkin::common::{
    ensure_no_duplicate_ticket_in_id_bect, get_best_pending_from_sync_or_main_bect,
    merge_latest_checkin_etdr,
};
use crate::models::TCOCmessages::{FE_COMMIT_IN, FE_COMMIT_IN_RESP};
use crate::models::TollCache::get_toll_lanes_by_toll_id_with_fallback;
use crate::models::ETDR::{
    clear_etdr_after_transaction_complete, get_latest_checkin_by_etag, save_etdr, ETDR,
};
use crate::services::service::Service;
use crate::services::TransportTransactionStageService;
use crate::utils::{bect_skip_account_check, normalize_etag, now_utc_db_string, timestamp_ms};
use aes;
use cbc;
use r2d2::Pool;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

/// Xử lý COMMIT (process): cập nhật stage, trả COMMIT_RESP.
pub(crate) async fn process_commit(
    fe_commit: &FE_COMMIT_IN,
    conn_id: i32,
    encryptor: &cbc::Encryptor<aes::Aes128>,
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
) -> Result<(Vec<u8>, i32), Box<dyn Error>> {
    tracing::debug!(
        conn_id,
        request_id = fe_commit.request_id,
        "[COMMIT] processing commit"
    );

    // Chuẩn hóa etag sớm để dùng khi lưu ETDR trước khi trả lỗi (vd. TOLL_LANE not found)
    let etag_norm = crate::utils::normalize_etag(&fe_commit.etag);

    let step_lanes = Instant::now();
    let lanes =
        get_toll_lanes_by_toll_id_with_fallback(fe_commit.station, db_pool.clone(), cache.clone())
            .await;
    let elapsed_ms = step_lanes.elapsed().as_millis() as u64;
    tracing::debug!(
        conn_id,
        request_id = fe_commit.request_id,
        elapsed_ms,
        step = "get_toll_lanes_by_toll_id_with_fallback",
        "[COMMIT] COMMIT external call finished"
    );
    let toll_lane = match lanes.iter().find(|l| l.lane_code == fe_commit.lane) {
        Some(l) => {
            tracing::debug!(conn_id, request_id = fe_commit.request_id, toll_lane_id = l.toll_lane_id, lane_code = %l.lane_code, "[COMMIT] TOLL_LANE cache hit");
            l.clone()
        }
        None => {
            tracing::error!(
                provider = "BECT",
                conn_id,
                request_id = fe_commit.request_id,
                toll_id = fe_commit.station,
                lane = fe_commit.lane,
                etag = %etag_norm,
                status = fe::NOT_FOUND_STATION_LANE,
                error_detail = crate::utils::fe_status_detail(fe::NOT_FOUND_STATION_LANE),
                detail = "TOLL_LANE not in cache",
                "[COMMIT] COMMIT TOLL_LANE not in cache"
            );
            // Lưu dữ liệu giao dịch (ETDR) trước khi trả về lỗi
            if let Some(mut etdr) = get_latest_checkin_by_etag(&etag_norm).await {
                let now_ms = timestamp_ms();
                etdr.time_update = now_ms;
                save_etdr(etdr);
                tracing::debug!(conn_id, request_id = fe_commit.request_id, etag = %etag_norm, "[COMMIT] ETDR cache updated before return TOLL_LANE error");
            }
            let mut fe_resp: FE_COMMIT_IN_RESP = FE_COMMIT_IN_RESP::default();
            fe_resp.message_length = fe_protocol::response_header_status_len();
            fe_resp.command_id = fe::COMMIT_RESP;
            fe_resp.request_id = fe_commit.request_id;
            fe_resp.session_id = conn_id as i64;
            fe_resp.status = fe::NOT_FOUND_STATION_LANE;
            let reply_bytes = serialize_and_encrypt_commit_response(&fe_resp, encryptor).await?;
            return Ok((reply_bytes, fe_resp.status));
        }
    };

    let lane_type_str = &toll_lane.lane_type;
    match lane_type_str.as_str() {
        lane_type::IN => {
            tracing::debug!(
                conn_id,
                request_id = fe_commit.request_id,
                lane_type = lane_type::IN,
                "[COMMIT] processing IN lane"
            );

            let now = now_utc_db_string();
            let transport_service = TransportTransactionStageService::default();
            let mut found_transport_trans_id = 0_i64;
            let mut record_found = false;
            let mut stage_for_in_update: Option<TransportTransactionStage> = None;

            // Lấy bản tin latest by etag (merge cache + sync + TTS), validate: không có hoặc đã có checkin_commit_datetime → lỗi không tìm thấy giao dịch.
            let step_merge = Instant::now();
            let cache_opt = get_latest_checkin_by_etag(&etag_norm).await;
            let db_opt = get_best_pending_from_sync_or_main_bect(&etag_norm).await;
            let etdr_opt = merge_latest_checkin_etdr(cache_opt, db_opt);
            let elapsed_ms = step_merge.elapsed().as_millis() as u64;
            tracing::debug!(conn_id, request_id = fe_commit.request_id, etag = %etag_norm, elapsed_ms, step = "resolve_etdr_merge_IN", "[COMMIT] COMMIT step finished");

            if let Some(ref etdr) = etdr_opt {
                if etdr.checkin_commit_datetime != 0
                    || etdr.checkin_datetime == 0
                    || etdr.checkout_datetime != 0
                    || etdr.checkout_commit_datetime != 0
                {
                    tracing::error!(
                        provider = "BECT",
                        conn_id,
                        request_id = fe_commit.request_id,
                        etag = %etag_norm,
                        ticket_id = etdr.ticket_id,
                        status = fe::NOT_FOUND_ROUTE_TRANSACTION,
                        error_detail = crate::utils::fe_status_detail(fe::NOT_FOUND_ROUTE_TRANSACTION),
                        detail = "transaction not found (already has checkin commit)",
                        "[COMMIT] COMMIT failed"
                    );
                    let mut fe_resp: FE_COMMIT_IN_RESP = FE_COMMIT_IN_RESP::default();
                    fe_resp.message_length = fe_protocol::response_header_status_len();
                    fe_resp.command_id = fe::COMMIT_RESP;
                    fe_resp.request_id = fe_commit.request_id;
                    fe_resp.session_id = conn_id as i64;
                    fe_resp.status = fe::NOT_FOUND_ROUTE_TRANSACTION;
                    let reply_bytes =
                        serialize_and_encrypt_commit_response(&fe_resp, encryptor).await?;
                    return Ok((reply_bytes, fe_resp.status));
                }
                found_transport_trans_id = etdr.ticket_id;
                if let Ok(Some(stage)) = transport_service.get_by_id(etdr.ticket_id).await {
                    if stage.checkin_commit_datetime.is_none() {
                        record_found = true;
                        stage_for_in_update = Some(stage);
                        tracing::debug!(
                            conn_id,
                            request_id = fe_commit.request_id,
                            transport_trans_id = found_transport_trans_id,
                            "[COMMIT] TRANSPORT_TRANSACTION_STAGE found by merge (cache+sync+TTS) for checkin commit"
                        );
                    } else {
                        tracing::error!(
                            conn_id,
                            request_id = fe_commit.request_id,
                            etag = %etag_norm,
                            transport_trans_id = etdr.ticket_id,
                            "[COMMIT] return error: transaction not found (record already has checkin_commit_datetime)"
                        );
                    }
                } else {
                    tracing::error!(
                        conn_id,
                        request_id = fe_commit.request_id,
                        transport_trans_id = etdr.ticket_id,
                        etag = %etag_norm,
                        "[COMMIT] TRANSPORT_TRANSACTION_STAGE get_by_id failed after merge"
                    );
                }
            } else {
                tracing::warn!(
                    conn_id,
                    request_id = fe_commit.request_id,
                    etag = %etag_norm,
                    station = fe_commit.station,
                    lane = fe_commit.lane,
                    "[COMMIT] no record by merge (cache+sync+TTS) for checkin commit"
                );
            }

            if !record_found {
                tracing::error!(
                    provider = "BECT",
                    conn_id,
                    request_id = fe_commit.request_id,
                    etag = %etag_norm,
                    status = fe::NOT_FOUND_ROUTE_TRANSACTION,
                    error_detail = crate::utils::fe_status_detail(fe::NOT_FOUND_ROUTE_TRANSACTION),
                    detail = "transaction not found (IN lane, no record from merge)",
                    "[COMMIT] COMMIT failed"
                );
                let mut fe_resp: FE_COMMIT_IN_RESP = FE_COMMIT_IN_RESP::default();
                fe_resp.message_length = fe_protocol::response_header_status_len();
                fe_resp.command_id = fe::COMMIT_RESP;
                fe_resp.request_id = fe_commit.request_id;
                fe_resp.session_id = conn_id as i64;
                fe_resp.status = fe::NOT_FOUND_ROUTE_TRANSACTION;
                let reply_bytes =
                    serialize_and_encrypt_commit_response(&fe_resp, encryptor).await?;
                return Ok((reply_bytes, fe_resp.status));
            }

            let repo = TransportTransactionStageRepository::new();

            let fe_weight = if fe_commit.weight > 0 {
                Some(fe_commit.weight as i64)
            } else {
                None
            };
            let fe_reason_id = if fe_commit.reason_id > 0 {
                Some(fe_commit.reason_id as i64)
            } else {
                None
            };
            let img_count = Some(fe_commit.image_count);
            let checkin_status = Some("2".to_string()); // PASS: đã commit checkin (theo Java CheckioStatus)

            tracing::debug!(
                conn_id,
                request_id = fe_commit.request_id,
                transport_trans_id = found_transport_trans_id,
                toll_id = fe_commit.station,
                lane_id = fe_commit.lane,
                img_count = fe_commit.image_count,
                "[COMMIT] updating CHECKIN COMMIT fields"
            );

            if let Some(ref stage) = stage_for_in_update {
                if let Some(tid) = stage.ticket_in_id {
                    if tid != 0 {
                        if let Err(dup_status) = ensure_no_duplicate_ticket_in_id_bect(
                            tid,
                            found_transport_trans_id,
                            fe_commit.request_id,
                            "[COMMIT]",
                        )
                        .await
                        {
                            tracing::error!(
                                conn_id,
                                request_id = fe_commit.request_id,
                                transport_trans_id = found_transport_trans_id,
                                ticket_in_id = tid,
                                "[COMMIT] COMMIT IN duplicate transaction, rejecting"
                            );
                            let mut fe_resp: FE_COMMIT_IN_RESP = FE_COMMIT_IN_RESP::default();
                            fe_resp.message_length = fe_protocol::response_header_status_len();
                            fe_resp.command_id = fe::COMMIT_RESP;
                            fe_resp.request_id = fe_commit.request_id;
                            fe_resp.session_id = conn_id as i64;
                            fe_resp.status = dup_status;
                            let reply_bytes =
                                serialize_and_encrypt_commit_response(&fe_resp, encryptor).await?;
                            return Ok((reply_bytes, fe_resp.status));
                        }
                    }
                }
            }

            match repo.update_checkin_commit(
                found_transport_trans_id,
                &now,
                fe_commit.image_count,
                &now,
                "P",
                fe_weight,
                fe_reason_id,
                img_count,
                checkin_status.as_deref(),
            ) {
                Ok(true) => {
                    tracing::info!(
                        conn_id,
                        request_id = fe_commit.request_id,
                        transport_trans_id = found_transport_trans_id,
                        station = fe_commit.station,
                        lane = fe_commit.lane,
                        "[COMMIT] TRANSPORT_TRANSACTION_STAGE CHECKIN commit updated"
                    );
                    // ETDR cache as source of truth: update cache from existing etdr (from merge cache+DB), do not overwrite from DB.
                    let now_ms = timestamp_ms();
                    if let Some(mut etdr) = etdr_opt.clone() {
                        etdr.checkin_commit_datetime = now_ms;
                        etdr.time_route_checkin_commit = now_ms;
                        etdr.time_update = now_ms;
                        if etdr.rating_details.is_empty() {
                            etdr.rating_details = get_rating_detail_cached(
                                cache.as_ref(),
                                found_transport_trans_id,
                                true,
                            )
                            .await;
                        }
                        etdr.db_saved = true;
                        save_etdr(etdr.clone());
                        tracing::debug!(conn_id, request_id = fe_commit.request_id, transport_trans_id = found_transport_trans_id, etag = %etag_norm, "[COMMIT] ETDR cache updated (source of truth, checkin commit)");
                        let payload = build_checkin_payload_from_etdr(
                            &etdr,
                            CheckinPayloadOverrides {
                                request_id: Some(fe_commit.request_id),
                                tid: Some(String::new()),
                                ticket_in_id: Some(found_transport_trans_id),
                                etag: Some(etag_norm.clone()),
                                plate: Some(fe_commit.plate.clone()),
                                station_in: Some(fe_commit.station),
                                lane_in: Some(fe_commit.lane),
                                checkin_commit_datetime: Some(now_ms),
                            },
                        );
                        send_checkin_to_kafka(payload);
                    } else {
                        // Cache miss: lấy từ DB rồi mới cập nhật cache và gửi Kafka
                        if let Ok(Some(db_record)) =
                            transport_service.get_by_id(found_transport_trans_id).await
                        {
                            let mut etdr_from_db =
                                ETDR::from_transport_transaction_stage(&db_record);
                            if etdr_from_db.rating_details.is_empty() {
                                etdr_from_db.rating_details = get_rating_detail_cached(
                                    cache.as_ref(),
                                    found_transport_trans_id,
                                    true,
                                )
                                .await;
                            }
                            etdr_from_db.db_saved = true;
                            save_etdr(etdr_from_db.clone());
                            let payload = build_checkin_payload_from_etdr(
                                &etdr_from_db,
                                CheckinPayloadOverrides {
                                    request_id: Some(fe_commit.request_id),
                                    tid: Some(String::new()),
                                    ticket_in_id: Some(found_transport_trans_id),
                                    etag: Some(etag_norm.clone()),
                                    plate: Some(fe_commit.plate.clone()),
                                    station_in: Some(fe_commit.station),
                                    lane_in: Some(fe_commit.lane),
                                    ..Default::default()
                                },
                            );
                            send_checkin_to_kafka(payload);
                            tracing::debug!(conn_id, request_id = fe_commit.request_id, transport_trans_id = found_transport_trans_id, etag = %etag_norm, "[COMMIT] ETDR cache filled from DB (cache was miss)");
                        }
                    }
                }
                Ok(false) => {
                    tracing::warn!(
                        conn_id,
                        request_id = fe_commit.request_id,
                        transport_trans_id = found_transport_trans_id,
                        "[COMMIT] update returned false, no rows affected"
                    );
                    // Vẫn update ETDR cache và gửi Kafka (reuse etdr from single merge fetch).
                    if let Some(mut etdr) = etdr_opt.clone() {
                        let now_ms = timestamp_ms();
                        etdr.checkin_commit_datetime = now_ms;
                        etdr.time_route_checkin_commit = now_ms;
                        etdr.time_update = now_ms;
                        let payload = build_checkin_payload_from_etdr(
                            &etdr,
                            CheckinPayloadOverrides {
                                request_id: Some(fe_commit.request_id),
                                tid: Some(String::new()),
                                ticket_in_id: Some(found_transport_trans_id),
                                etag: Some(etag_norm.clone()),
                                plate: Some(fe_commit.plate.clone()),
                                station_in: Some(fe_commit.station),
                                lane_in: Some(fe_commit.lane),
                                checkin_commit_datetime: Some(now_ms),
                            },
                        );
                        save_etdr(etdr);
                        tracing::debug!(conn_id, request_id = fe_commit.request_id, transport_trans_id = found_transport_trans_id, etag = %etag_norm, "[COMMIT] ETDR cache updated despite no rows affected");
                        send_checkin_to_kafka(payload);
                    }
                }
                Err(e) => {
                    tracing::error!(conn_id, request_id = fe_commit.request_id, error = %e, "[COMMIT] TRANSPORT_TRANSACTION_STAGE update failed");
                    // Vẫn update ETDR cache và gửi Kafka (reuse etdr from single merge fetch).
                    if let Some(mut etdr) = etdr_opt.clone() {
                        let now_ms = timestamp_ms();
                        etdr.checkin_commit_datetime = now_ms;
                        etdr.time_route_checkin_commit = now_ms;
                        etdr.time_update = now_ms;
                        let payload = build_checkin_payload_from_etdr(
                            &etdr,
                            CheckinPayloadOverrides {
                                request_id: Some(fe_commit.request_id),
                                tid: Some(String::new()),
                                ticket_in_id: Some(found_transport_trans_id),
                                etag: Some(etag_norm.clone()),
                                plate: Some(fe_commit.plate.clone()),
                                station_in: Some(fe_commit.station),
                                lane_in: Some(fe_commit.lane),
                                checkin_commit_datetime: Some(now_ms),
                            },
                        );
                        save_etdr(etdr);
                        tracing::debug!(conn_id, request_id = fe_commit.request_id, transport_trans_id = found_transport_trans_id, etag = %etag_norm, "[COMMIT] ETDR cache updated despite DB update failure");
                        send_checkin_to_kafka(payload);
                    }
                }
            }
        }
        lane_type::OUT => {
            tracing::debug!(
                conn_id,
                request_id = fe_commit.request_id,
                lane_type = lane_type::OUT,
                "[COMMIT] processing OUT lane"
            );

            let transport_trans_id = fe_commit.ticket_id;
            // Single ETDR fetch for all OUT response updates (fetch once before verify/update).
            // When ticket_id==0 we get ETDR from merge below and set etdr_for_out_update there to avoid double fetch.
            let mut etdr_for_out_update = if transport_trans_id != 0 {
                get_latest_checkin_by_etag(&etag_norm).await
            } else {
                None
            };

            let now = now_utc_db_string();

            let transport_service = TransportTransactionStageService::default();
            let mut found_transport_trans_id = transport_trans_id;
            let mut record_found = false;
            let mut current_record_for_update: Option<TransportTransactionStage> = None;

            if transport_trans_id == 0 {
                tracing::warn!(
                    conn_id,
                    request_id = fe_commit.request_id,
                    "[COMMIT] ticket_id=0, finding latest by etag (cache + sync + TTS)"
                );
                let step_merge_out = Instant::now();
                let cache_opt = get_latest_checkin_by_etag(&etag_norm).await;
                let db_opt = get_best_pending_from_sync_or_main_bect(&etag_norm).await;
                let etdr_opt = merge_latest_checkin_etdr(cache_opt, db_opt);
                let elapsed_ms = step_merge_out.elapsed().as_millis() as u64;
                tracing::debug!(conn_id, request_id = fe_commit.request_id, etag = %etag_norm, elapsed_ms, step = "resolve_etdr_merge_OUT", "[COMMIT] COMMIT step finished");
                if let Some(etdr) = etdr_opt {
                    let valid_for_commit = etdr.checkout_datetime != 0
                        && etdr.checkout_commit_datetime == 0
                        && etdr.checkin_datetime != 0
                        && etdr.checkin_commit_datetime != 0;
                    if valid_for_commit {
                        found_transport_trans_id = etdr.ticket_id;
                        if let Ok(Some(stage)) = transport_service.get_by_id(etdr.ticket_id).await {
                            current_record_for_update = Some(stage);
                            record_found = true;
                            etdr_for_out_update = Some(etdr.clone());
                            tracing::debug!(
                                conn_id,
                                request_id = fe_commit.request_id,
                                transport_trans_id = found_transport_trans_id,
                                "[COMMIT] TRANSPORT_TRANSACTION_STAGE found by merge (cache+sync+TTS)"
                            );
                        } else {
                            tracing::error!(
                                conn_id,
                                request_id = fe_commit.request_id,
                                transport_trans_id = etdr.ticket_id,
                                "[COMMIT] TRANSPORT_TRANSACTION_STAGE get_by_id failed after merge"
                            );
                        }
                    } else {
                        tracing::error!(
                            conn_id,
                            request_id = fe_commit.request_id,
                            etag = %etag_norm,
                            ticket_id = etdr.ticket_id,
                            has_checkout_dt = etdr.checkout_datetime != 0,
                            has_commit_dt = etdr.checkout_commit_datetime != 0,
                            "[COMMIT] return error: transaction not found (no checkout time or already has commit)"
                        );
                        let mut fe_resp: FE_COMMIT_IN_RESP = FE_COMMIT_IN_RESP::default();
                        fe_resp.message_length = fe_protocol::response_header_status_len();
                        fe_resp.command_id = fe::COMMIT_RESP;
                        fe_resp.request_id = fe_commit.request_id;
                        fe_resp.session_id = conn_id as i64;
                        fe_resp.status = fe::NOT_FOUND_ROUTE_TRANSACTION;
                        let reply_bytes =
                            serialize_and_encrypt_commit_response(&fe_resp, encryptor).await?;
                        return Ok((reply_bytes, fe_resp.status));
                    }
                } else {
                    tracing::warn!(conn_id, request_id = fe_commit.request_id, etag = %etag_norm, station = fe_commit.station, lane = fe_commit.lane, "[COMMIT] no record by merge (cache+sync+TTS)");
                }
            } else {
                for retry_count in 0..=commit_retry::MAX_RETRIES {
                    match transport_service.get_by_id(transport_trans_id).await {
                        Ok(Some(stage)) => {
                            tracing::debug!(
                                conn_id,
                                request_id = fe_commit.request_id,
                                transport_trans_id,
                                retry = retry_count,
                                "[COMMIT] TRANSPORT_TRANSACTION_STAGE found"
                            );
                            current_record_for_update = Some(stage.clone());
                            record_found = true;
                            break;
                        }
                        Ok(None) => {
                            if retry_count < commit_retry::MAX_RETRIES {
                                let delay_ms =
                                    commit_retry::INITIAL_RETRY_DELAY_MS * (retry_count as u64 + 1);
                                tracing::debug!(
                                    conn_id,
                                    request_id = fe_commit.request_id,
                                    transport_trans_id,
                                    delay_ms,
                                    attempt = retry_count + 1,
                                    max = commit_retry::MAX_RETRIES,
                                    "[COMMIT] TRANSPORT_TRANSACTION_STAGE not found, retrying"
                                );
                                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms))
                                    .await;
                            } else {
                                tracing::warn!(conn_id, request_id = fe_commit.request_id, transport_trans_id, retries = commit_retry::MAX_RETRIES, "[COMMIT] TRANSPORT_TRANSACTION_STAGE not found after retries, fallback by merge (cache+sync+TTS)");
                                let cache_opt = get_latest_checkin_by_etag(&etag_norm).await;
                                let db_opt =
                                    get_best_pending_from_sync_or_main_bect(&etag_norm).await;
                                let etdr_opt = merge_latest_checkin_etdr(cache_opt, db_opt);
                                if let Some(etdr) = etdr_opt {
                                    let valid_for_commit = etdr.checkout_datetime != 0
                                        && etdr.checkout_commit_datetime == 0
                                        && etdr.checkin_datetime != 0
                                        && etdr.checkin_commit_datetime != 0;
                                    if valid_for_commit {
                                        if let Ok(Some(stage)) =
                                            transport_service.get_by_id(etdr.ticket_id).await
                                        {
                                            found_transport_trans_id = etdr.ticket_id;
                                            current_record_for_update = Some(stage);
                                            record_found = true;
                                            tracing::debug!(conn_id, request_id = fe_commit.request_id, transport_trans_id = found_transport_trans_id, original_ticket_id = transport_trans_id, "[COMMIT] TRANSPORT_TRANSACTION_STAGE found by merge fallback");
                                        }
                                    } else {
                                        tracing::error!(conn_id, request_id = fe_commit.request_id, etag = %etag_norm, ticket_id = etdr.ticket_id, has_checkout_dt = etdr.checkout_datetime != 0, has_commit_dt = etdr.checkout_commit_datetime != 0, "[COMMIT] return error: transaction not found");
                                        let mut fe_resp: FE_COMMIT_IN_RESP =
                                            FE_COMMIT_IN_RESP::default();
                                        fe_resp.message_length =
                                            fe_protocol::response_header_status_len();
                                        fe_resp.command_id = fe::COMMIT_RESP;
                                        fe_resp.request_id = fe_commit.request_id;
                                        fe_resp.session_id = conn_id as i64;
                                        fe_resp.status = fe::NOT_FOUND_ROUTE_TRANSACTION;
                                        let reply_bytes = serialize_and_encrypt_commit_response(
                                            &fe_resp, encryptor,
                                        )
                                        .await?;
                                        return Ok((reply_bytes, fe_resp.status));
                                    }
                                } else {
                                    tracing::error!(conn_id, request_id = fe_commit.request_id, transport_trans_id, etag = %etag_norm, "[COMMIT] TRANSPORT_TRANSACTION_STAGE not found by ticket_id or merge");
                                }
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::error!(conn_id, request_id = fe_commit.request_id, error = %e, "[COMMIT] query TRANSPORT_TRANSACTION_STAGE by id failed");
                            break;
                        }
                    }
                }
            }

            if !record_found {
                tracing::error!(
                    provider = "BECT",
                    conn_id,
                    request_id = fe_commit.request_id,
                    transport_trans_id,
                    etag = %etag_norm,
                    status = fe::NOT_FOUND_ROUTE_TRANSACTION,
                    error_detail = crate::utils::fe_status_detail(fe::NOT_FOUND_ROUTE_TRANSACTION),
                    detail = "transaction not found (OUT lane)",
                    "[COMMIT] COMMIT failed"
                );
                let mut fe_resp: FE_COMMIT_IN_RESP = FE_COMMIT_IN_RESP::default();
                fe_resp.message_length = fe_protocol::response_header_status_len();
                fe_resp.command_id = fe::COMMIT_RESP;
                fe_resp.request_id = fe_commit.request_id;
                fe_resp.session_id = conn_id as i64;
                fe_resp.status = fe::NOT_FOUND_ROUTE_TRANSACTION;
                let reply_bytes =
                    serialize_and_encrypt_commit_response(&fe_resp, encryptor).await?;
                return Ok((reply_bytes, fe_resp.status));
            }

            let checkout_datetime_to_set = match &current_record_for_update {
                Some(record) => {
                    if record.checkout_datetime.is_none() {
                        Some(now.as_str())
                    } else {
                        None
                    }
                }
                None => Some(now.as_str()),
            };

            let img_count = match &current_record_for_update {
                Some(record) => {
                    if let Some(checkin_img_count) = record.checkin_img_count {
                        Some(checkin_img_count + fe_commit.image_count)
                    } else {
                        Some(fe_commit.image_count)
                    }
                }
                None => Some(fe_commit.image_count),
            };

            // Kiểm tra tài khoản (có thể bypass bằng env). Không return ngay: luôn ghi DB rồi mới trả lỗi.
            let mut error_status = 0_i32;
            if !bect_skip_account_check() {
                let pool_clone = RATING_DB.clone();
                let etag_clone = etag_norm.clone();
                let sub_opt = tokio::task::spawn_blocking(move || {
                    get_subscriber_by_etag(&pool_clone, &etag_clone)
                })
                .await
                .unwrap_or(Ok(None));
                match &sub_opt {
                    Ok(None) => {
                        tracing::warn!(conn_id, request_id = fe_commit.request_id, etag = %etag_norm, "[COMMIT] commit account check: subscriber not found");
                        error_status = bect::SUBSCRIBER_NOT_FOUND;
                    }
                    Ok(Some(sub)) => {
                        if sub.account_id <= 0 {
                            tracing::warn!(conn_id, request_id = fe_commit.request_id, etag = %etag_norm, account_id = sub.account_id, "[COMMIT] commit account check: account not found or inactive");
                            error_status = bect::ACCOUNT_IS_NOT_ACTIVE;
                        }
                    }
                    Err(_) => {}
                }
            }

            let charge_status = if error_status != 0 { "FALL" } else { "SUCC" };
            // checkout_status per Java CheckioStatus: "1" = CHECKIN, "2" = PASS. Commit thành công → PASS; lỗi trừ tiền → giữ CHECKIN.
            let checkout_status = if error_status != 0 {
                Some("1")
            } else {
                Some("2")
            };

            // Cập nhật trạng thái giao dịch ACCOUNT_TRANSACTION (checkout commit): lấy giao dịch chưa commit, set AUTOCOMMIT_STATUS và FINISH_DATETIME.
            let account_trans_id = current_record_for_update
                .as_ref()
                .and_then(|r| r.account_trans_id)
                .unwrap_or(0);
            if account_trans_id > 0 {
                let pool_at = RATING_DB.clone();
                let id_at = account_trans_id;
                let trans_opt = tokio::task::spawn_blocking(move || {
                    get_account_transaction_for_commit(&pool_at, id_at)
                })
                .await
                .unwrap_or(Ok(None));
                if let Ok(Some(trans)) = trans_opt {
                    if !allow_commit_or_rollback(trans.autocommit, trans.autocommit_status) {
                        tracing::debug!(conn_id, request_id = fe_commit.request_id, account_transaction_id = account_trans_id, autocommit = ?trans.autocommit, autocommit_status = ?trans.autocommit_status, "[COMMIT] ACCOUNT_TRANSACTION skip update (already committed/rollback or autocommit)");
                    } else {
                        let (autocommit_status, new_balance, new_available_bal, old_rol, new_rol) =
                            if error_status == 0 {
                                (
                                    AUTOCOMMIT_STATUS_SUCCESS,
                                    trans.old_balance - trans.amount,
                                    trans.old_available_bal - trans.amount,
                                    None,
                                    None,
                                )
                            } else {
                                (
                                    AUTOCOMMIT_STATUS_FAIL,
                                    trans.old_balance,
                                    trans.old_available_bal,
                                    Some(trans.old_available_bal),
                                    Some(trans.old_available_bal),
                                )
                            };
                        let pool_up = RATING_DB.clone();
                        let now_clone = now.clone();
                        let at_id = account_trans_id;
                        let res = tokio::task::spawn_blocking(move || {
                            update_account_transaction_commit_status(
                                &pool_up,
                                at_id,
                                autocommit_status,
                                &now_clone,
                                new_balance,
                                new_available_bal,
                                old_rol,
                                new_rol,
                            )
                        })
                        .await;
                        match res {
                            Ok(Ok(true)) => {
                                tracing::debug!(
                                    conn_id,
                                    request_id = fe_commit.request_id,
                                    account_transaction_id = account_trans_id,
                                    autocommit_status,
                                    "[COMMIT] ACCOUNT_TRANSACTION commit status updated"
                                );
                            }
                            Ok(Err(e)) => {
                                tracing::error!(conn_id, request_id = fe_commit.request_id, account_transaction_id = account_trans_id, error = %e, "[COMMIT] ACCOUNT_TRANSACTION update_commit_status failed");
                            }
                            _ => {}
                        }
                    }
                }
            }

            tracing::debug!(
                conn_id,
                request_id = fe_commit.request_id,
                transport_trans_id = found_transport_trans_id,
                toll_id = fe_commit.station,
                lane_id = fe_commit.lane,
                img_count = fe_commit.image_count,
                amount = fe_commit.transaction_amount,
                charge_status,
                "[COMMIT] updating CHECKOUT COMMIT fields"
            );

            let repo = TransportTransactionStageRepository::new();
            let fe_reason_id = if fe_commit.reason_id > 0 {
                Some(fe_commit.reason_id as i64)
            } else {
                None
            };
            let checkout_plate_status_commit = if error_status != 0 { Some("0") } else { None };
            // BKS: ưu tiên từ FE commit; checkout FE thường không gửi plate → fallback từ bản ghi DB (checkout_plate từ checkin out, checkin_plate, plate)
            let plate_from_fe = normalize_etag(&fe_commit.plate);
            let effective_checkout_plate: String = if !plate_from_fe.is_empty() {
                plate_from_fe.to_string()
            } else {
                current_record_for_update
                    .as_ref()
                    .and_then(|r| {
                        r.checkout_plate
                            .as_deref()
                            .map(normalize_etag)
                            .filter(|s| !s.is_empty())
                    })
                    .or_else(|| {
                        current_record_for_update.as_ref().and_then(|r| {
                            r.checkin_plate
                                .as_deref()
                                .map(normalize_etag)
                                .filter(|s| !s.is_empty())
                        })
                    })
                    .or_else(|| {
                        current_record_for_update.as_ref().and_then(|r| {
                            r.plate
                                .as_deref()
                                .map(normalize_etag)
                                .filter(|s| !s.is_empty())
                        })
                    })
                    .unwrap_or_else(|| "0000000000".to_string())
            };
            let checkout_plate_for_db: &str = if effective_checkout_plate.is_empty() {
                "0000000000"
            } else {
                &effective_checkout_plate
            };
            match repo.update_checkout_commit(
                found_transport_trans_id,
                &now,
                fe_commit.image_count,
                &now,
                charge_status,
                checkout_status,
                fe_reason_id,
                checkout_datetime_to_set,
                img_count,
                checkout_plate_status_commit,
                Some(checkout_plate_for_db),
                if error_status == 0 { Some(0) } else { None }, // sync_status = 0 (chưa đồng bộ) khi commit giao dịch
            ) {
                Ok(true) => {
                    tracing::info!(
                        conn_id,
                        request_id = fe_commit.request_id,
                        transport_trans_id = found_transport_trans_id,
                        station = fe_commit.station,
                        lane = fe_commit.lane,
                        charge_status,
                        "[COMMIT] TRANSPORT_TRANSACTION_STAGE CHECKOUT commit updated"
                    );
                    // Luôn load lại từ DB và update ETDR cache (dù SUCC hay FALL)
                    let transport_service = TransportTransactionStageService::default();
                    match transport_service.get_by_id(found_transport_trans_id).await {
                        Ok(Some(db_record)) => {
                            let mut etdr_from_db =
                                ETDR::from_transport_transaction_stage(&db_record);
                            // Load rating_details một lần (từ cache hoặc TCD/DB) và tái sử dụng cho Kafka.
                            let rating_details_loaded = if etdr_from_db.rating_details.is_empty() {
                                tracing::debug!(
                                    conn_id,
                                    request_id = fe_commit.request_id,
                                    transport_trans_id = found_transport_trans_id,
                                    "[COMMIT] rating_details empty, loading from TCD"
                                );
                                get_rating_detail_cached(
                                    cache.as_ref(),
                                    found_transport_trans_id,
                                    true,
                                )
                                .await
                            } else {
                                etdr_from_db.rating_details.clone()
                            };
                            etdr_from_db.rating_details = rating_details_loaded.clone();
                            let rating_details_for_kafka = rating_details_loaded;

                            // Gửi checkout event lên Kafka chỉ khi commit success (error_status == 0), trước khi clear ETDR
                            if error_status == 0 {
                                if let Some(etdr) = etdr_for_out_update.clone() {
                                    let now_ms = timestamp_ms();
                                    // Các datetime: checkin = lúc CHECKIN in, checkin_commit = lúc COMMIT in, checkout = lúc CHECKIN out, checkout_commit = lúc COMMIT out (now)
                                    let checkin_dt_ms = etdr.checkin_datetime;
                                    let checkin_commit_ms = if etdr.checkin_commit_datetime <= 0 {
                                        now_ms
                                    } else {
                                        etdr.checkin_commit_datetime
                                    };
                                    let checkout_dt_ms = etdr.checkout_datetime;

                                    let payload = build_checkout_payload_from_etdr(
                                        &etdr,
                                        CheckoutPayloadOverrides {
                                            request_id: Some(fe_commit.request_id),
                                            tid: Some(String::new()),
                                            ticket_in_id: Some(found_transport_trans_id),
                                            ticket_out_id: Some(found_transport_trans_id),
                                            etag: Some(etag_norm.clone()),
                                            plate: Some(effective_checkout_plate.clone()),
                                            station_in: Some(etdr.station_id),
                                            lane_in: Some(etdr.lane_id),
                                            checkin_datetime: Some(checkin_dt_ms),
                                            checkin_commit_datetime: Some(checkin_commit_ms),
                                            station_out: Some(fe_commit.station),
                                            lane_out: Some(fe_commit.lane),
                                            checkout_datetime: Some(checkout_dt_ms),
                                            checkout_commit_datetime: Some(now_ms),
                                            trans_amount: Some(
                                                db_record.total_amount.unwrap_or(0) as i32
                                            ),
                                            rating_detail: Some(rating_details_for_kafka.clone()),
                                            ..Default::default()
                                        },
                                    );
                                    send_checkout_to_kafka(payload);
                                } else {
                                    // Commit success and saved to DB: send Kafka from DB record when ETDR cache miss
                                    let now_ms = timestamp_ms();
                                    let payload = build_checkout_payload_from_etdr(
                                        &etdr_from_db,
                                        CheckoutPayloadOverrides {
                                            request_id: Some(fe_commit.request_id),
                                            tid: Some(String::new()),
                                            ticket_in_id: Some(found_transport_trans_id),
                                            ticket_out_id: Some(found_transport_trans_id),
                                            etag: Some(etag_norm.clone()),
                                            plate: Some(effective_checkout_plate.clone()),
                                            station_out: Some(fe_commit.station),
                                            lane_out: Some(fe_commit.lane),
                                            checkout_commit_datetime: Some(now_ms),
                                            trans_amount: Some(
                                                db_record.total_amount.unwrap_or(0) as i32
                                            ),
                                            rating_detail: Some(rating_details_for_kafka),
                                            ..Default::default()
                                        },
                                    );
                                    send_checkout_to_kafka(payload);
                                    tracing::debug!(
                                        conn_id, request_id = fe_commit.request_id,
                                        etag = %etag_norm,
                                        "[COMMIT] checkout event sent to Kafka (built from DB, ETDR cache miss)"
                                    );
                                }
                            }
                            // Transaction completed: remove ETDR from cache and KeyDB
                            clear_etdr_after_transaction_complete(
                                &etag_norm,
                                Some(found_transport_trans_id),
                            )
                            .await;
                            invalidate_tcd_rating_cache(
                                cache.as_ref(),
                                found_transport_trans_id,
                                true,
                            )
                            .await;
                            tracing::debug!(conn_id, request_id = fe_commit.request_id, transport_trans_id = found_transport_trans_id, etag = %etag_norm, "[COMMIT] ETDR cleared after checkout commit");
                        }
                        Ok(None) | Err(_) => {
                            // Nếu không load được từ DB, vẫn update ETDR cache và gửi Kafka (reuse single fetch).
                            if let Some(mut etdr) = etdr_for_out_update.clone() {
                                let now_ms = timestamp_ms();
                                etdr.checkout_datetime = now_ms;
                                etdr.time_route_checkout = now_ms;
                                etdr.checkout_commit_datetime = now_ms;
                                etdr.toll_out = fe_commit.station;
                                etdr.lane_out = fe_commit.lane;
                                etdr.time_update = now_ms;
                                let rating_details = if etdr.rating_details.is_empty() {
                                    get_rating_detail_cached(
                                        cache.as_ref(),
                                        found_transport_trans_id,
                                        true,
                                    )
                                    .await
                                } else {
                                    etdr.rating_details.clone()
                                };
                                let payload = build_checkout_payload_from_etdr(
                                    &etdr,
                                    CheckoutPayloadOverrides {
                                        request_id: Some(fe_commit.request_id),
                                        tid: Some(String::new()),
                                        ticket_in_id: Some(found_transport_trans_id),
                                        ticket_out_id: Some(found_transport_trans_id),
                                        etag: Some(etag_norm.clone()),
                                        plate: Some(effective_checkout_plate.clone()),
                                        station_out: Some(fe_commit.station),
                                        lane_out: Some(fe_commit.lane),
                                        checkout_datetime: Some(now_ms),
                                        checkout_commit_datetime: Some(now_ms),
                                        rating_detail: Some(rating_details),
                                        ..Default::default()
                                    },
                                );
                                if error_status == 0 {
                                    etdr.sync_status = 0;
                                }
                                save_etdr(etdr);
                                tracing::warn!(conn_id, request_id = fe_commit.request_id, transport_trans_id = found_transport_trans_id, etag = %etag_norm, "[COMMIT] ETDR cache updated (DB record not found after update)");
                                send_checkout_to_kafka(payload);
                            }
                        }
                    }
                }
                Ok(false) => {
                    tracing::warn!(
                        conn_id,
                        request_id = fe_commit.request_id,
                        transport_trans_id = found_transport_trans_id,
                        "[COMMIT] update returned false, no rows affected"
                    );
                    // Vẫn update ETDR cache và gửi Kafka (reuse single fetch).
                    if let Some(mut etdr) = etdr_for_out_update.clone() {
                        let now_ms = timestamp_ms();
                        etdr.checkout_datetime = now_ms;
                        etdr.time_route_checkout = now_ms;
                        etdr.toll_out = fe_commit.station;
                        etdr.lane_out = fe_commit.lane;
                        etdr.time_update = now_ms;
                        let rating_details = if etdr.rating_details.is_empty() {
                            get_rating_detail_cached(cache.as_ref(), found_transport_trans_id, true)
                                .await
                        } else {
                            etdr.rating_details.clone()
                        };
                        let payload = build_checkout_payload_from_etdr(
                            &etdr,
                            CheckoutPayloadOverrides {
                                request_id: Some(fe_commit.request_id),
                                tid: Some(String::new()),
                                ticket_in_id: Some(found_transport_trans_id),
                                ticket_out_id: Some(found_transport_trans_id),
                                etag: Some(etag_norm.clone()),
                                plate: Some(effective_checkout_plate.clone()),
                                station_out: Some(fe_commit.station),
                                lane_out: Some(fe_commit.lane),
                                checkout_datetime: Some(now_ms),
                                checkout_commit_datetime: Some(now_ms),
                                rating_detail: Some(rating_details),
                                ..Default::default()
                            },
                        );
                        if error_status == 0 {
                            etdr.sync_status = 0;
                        }
                        save_etdr(etdr);
                        tracing::debug!(conn_id, request_id = fe_commit.request_id, transport_trans_id = found_transport_trans_id, etag = %etag_norm, "[COMMIT] ETDR cache updated despite no rows affected");
                        send_checkout_to_kafka(payload);
                    }
                }
                Err(e) => {
                    tracing::error!(conn_id, request_id = fe_commit.request_id, error = %e, "[COMMIT] TRANSPORT_TRANSACTION_STAGE update failed");
                    // Vẫn update ETDR cache và gửi Kafka (reuse single fetch).
                    if let Some(mut etdr) = etdr_for_out_update.clone() {
                        let now_ms = timestamp_ms();
                        etdr.checkout_datetime = now_ms;
                        etdr.time_route_checkout = now_ms;
                        etdr.toll_out = fe_commit.station;
                        etdr.lane_out = fe_commit.lane;
                        etdr.time_update = now_ms;
                        let rating_details = if etdr.rating_details.is_empty() {
                            get_rating_detail_cached(cache.as_ref(), found_transport_trans_id, true)
                                .await
                        } else {
                            etdr.rating_details.clone()
                        };
                        let payload = build_checkout_payload_from_etdr(
                            &etdr,
                            CheckoutPayloadOverrides {
                                request_id: Some(fe_commit.request_id),
                                tid: Some(String::new()),
                                ticket_in_id: Some(found_transport_trans_id),
                                ticket_out_id: Some(found_transport_trans_id),
                                etag: Some(etag_norm.clone()),
                                plate: Some(effective_checkout_plate.clone()),
                                station_out: Some(fe_commit.station),
                                lane_out: Some(fe_commit.lane),
                                checkout_datetime: Some(now_ms),
                                checkout_commit_datetime: Some(now_ms),
                                rating_detail: Some(rating_details),
                                ..Default::default()
                            },
                        );
                        if error_status == 0 {
                            etdr.sync_status = 0;
                        }
                        save_etdr(etdr);
                        tracing::warn!(
                            conn_id, request_id = fe_commit.request_id, transport_trans_id = found_transport_trans_id, etag = %etag_norm, "[COMMIT] ETDR cache updated despite DB update failure"
                        );
                        send_checkout_to_kafka(payload);
                    }
                }
            }

            // Sau khi đã ghi DB (và ETDR): nếu có lỗi kiểm tra tài khoản/số dư thì trả về lỗi
            if error_status != 0 {
                tracing::error!(
                    provider = "BECT",
                    conn_id,
                    request_id = fe_commit.request_id,
                    etag = %etag_norm,
                    transport_trans_id = found_transport_trans_id,
                    status = error_status,
                    error_detail = crate::utils::fe_status_detail(error_status),
                    flow = "COMMIT",
                    detail = "account check failed (subscriber not found or account inactive)",
                    "[COMMIT] COMMIT failed"
                );
                let mut fe_resp: FE_COMMIT_IN_RESP = FE_COMMIT_IN_RESP::default();
                fe_resp.message_length = fe_protocol::response_header_status_len();
                fe_resp.command_id = fe::COMMIT_RESP;
                fe_resp.request_id = fe_commit.request_id;
                fe_resp.session_id = conn_id as i64;
                fe_resp.status = error_status;
                let reply_bytes =
                    serialize_and_encrypt_commit_response(&fe_resp, encryptor).await?;
                return Ok((reply_bytes, fe_resp.status));
            }
        }
        _ => {
            tracing::error!(
                provider = "BECT",
                conn_id,
                request_id = fe_commit.request_id,
                lane_type = %lane_type_str,
                etag = %etag_norm,
                status = fe::NOT_FOUND_STATION_LANE,
                error_detail = crate::utils::fe_status_detail(fe::NOT_FOUND_STATION_LANE),
                detail = "invalid lane_type",
                "[COMMIT] COMMIT failed"
            );
            let mut fe_resp: FE_COMMIT_IN_RESP = FE_COMMIT_IN_RESP::default();
            fe_resp.message_length = fe_protocol::response_header_status_len();
            fe_resp.command_id = fe::COMMIT_RESP;
            fe_resp.request_id = fe_commit.request_id;
            fe_resp.session_id = conn_id as i64;
            fe_resp.status = fe::NOT_FOUND_STATION_LANE;
            let reply_bytes = serialize_and_encrypt_commit_response(&fe_resp, encryptor).await?;
            return Ok((reply_bytes, fe_resp.status));
        }
    }

    let mut fe_resp: FE_COMMIT_IN_RESP = FE_COMMIT_IN_RESP::default();
    fe_resp.message_length = fe_protocol::response_header_status_len();
    fe_resp.command_id = fe::COMMIT_RESP;
    fe_resp.request_id = fe_commit.request_id;
    fe_resp.session_id = conn_id as i64;
    fe_resp.status = 0;

    tracing::debug!(
        conn_id,
        request_id = fe_commit.request_id,
        status = fe_resp.status,
        "[COMMIT] commit response"
    );
    let step_serialize = Instant::now();
    let reply_bytes = serialize_and_encrypt_commit_response(&fe_resp, encryptor).await?;
    let elapsed_ms = step_serialize.elapsed().as_millis() as u64;
    tracing::debug!(
        conn_id,
        request_id = fe_commit.request_id,
        elapsed_ms,
        step = "serialize_and_encrypt_commit_response",
        "[COMMIT] COMMIT step finished"
    );
    tracing::debug!(
        conn_id,
        request_id = fe_commit.request_id,
        reply_len = reply_bytes.len(),
        "[COMMIT] sending FE_COMMIT_IN_RESP"
    );

    Ok((reply_bytes, fe_resp.status))
}
