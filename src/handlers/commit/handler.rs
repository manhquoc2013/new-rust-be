//! Handler COMMIT: cập nhật TRANSPORT_TRANSACTION_STAGE, trả FE COMMIT_RESP (resp).
//! Chỉ xử lý lane IN (commit check-in). Lane OUT bị từ chối, FE dùng CHECKOUT_COMMIT_BOO.

use super::common::{get_rating_detail_cached, serialize_and_encrypt_commit_response};
use crate::cache::config::cache_manager::CacheManager;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::constants::{fe, lane_type};
use crate::db::repositories::transport_transaction_stage::{
    TransportTransactionStage, TransportTransactionStageRepository,
};
use crate::fe_protocol;
use crate::handlers::checkin::common::{
    ensure_no_duplicate_ticket_in_id_bect, get_best_pending_from_sync_or_main_bect,
    merge_latest_checkin_etdr,
};
use crate::models::TCOCmessages::{FE_COMMIT_IN, FE_COMMIT_IN_RESP};
use crate::models::TollCache::get_toll_lanes_by_toll_id_with_fallback;
use crate::models::ETDR::{get_latest_checkin_by_etag, save_etdr, ETDR};
use crate::services::service::Service;
use crate::services::TransportTransactionStageService;
use crate::utils::{now_utc_db_string, timestamp_ms};
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
                    } else {
                        // Cache miss: lấy từ DB rồi cập nhật cache
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
                    // Vẫn update ETDR cache (reuse etdr from single merge fetch).
                    if let Some(mut etdr) = etdr_opt.clone() {
                        let now_ms = timestamp_ms();
                        etdr.checkin_commit_datetime = now_ms;
                        etdr.time_route_checkin_commit = now_ms;
                        etdr.time_update = now_ms;
                        save_etdr(etdr);
                        tracing::debug!(conn_id, request_id = fe_commit.request_id, transport_trans_id = found_transport_trans_id, etag = %etag_norm, "[COMMIT] ETDR cache updated despite no rows affected");
                    }
                }
                Err(e) => {
                    tracing::error!(conn_id, request_id = fe_commit.request_id, error = %e, "[COMMIT] TRANSPORT_TRANSACTION_STAGE update failed");
                    // Vẫn update ETDR cache (reuse etdr from single merge fetch).
                    if let Some(mut etdr) = etdr_opt.clone() {
                        let now_ms = timestamp_ms();
                        etdr.checkin_commit_datetime = now_ms;
                        etdr.time_route_checkin_commit = now_ms;
                        etdr.time_update = now_ms;
                        save_etdr(etdr);
                        tracing::debug!(conn_id, request_id = fe_commit.request_id, transport_trans_id = found_transport_trans_id, etag = %etag_norm, "[COMMIT] ETDR cache updated despite DB update failure");
                    }
                }
            }
        }
        lane_type::OUT => {
            tracing::warn!(
                conn_id,
                request_id = fe_commit.request_id,
                station = fe_commit.station,
                lane = fe_commit.lane,
                "[Commit] OUT lane rejected: use CHECKOUT_COMMIT_BOO for exit"
            );
            let mut fe_resp: FE_COMMIT_IN_RESP = FE_COMMIT_IN_RESP::default();
            fe_resp.message_length = fe_protocol::response_header_status_len();
            fe_resp.command_id = fe::COMMIT_RESP;
            fe_resp.request_id = fe_commit.request_id;
            fe_resp.session_id = conn_id as i64;
            fe_resp.status = fe::NOT_FOUND_ROUTE_TRANSACTION;
            let reply_bytes = serialize_and_encrypt_commit_response(&fe_resp, encryptor).await?;
            return Ok((reply_bytes, fe_resp.status));
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
