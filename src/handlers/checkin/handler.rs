//! Handler CHECKIN: FE gửi CHECKIN (req), handler tính phí, lưu DB, trả FE_CHECKIN_IN_RESP (resp).
//! Có kiểm tra tài khoản (tồn tại/active); không kiểm tra số dư. Bật/tắt bằng env BECT_SKIP_ACCOUNT_CHECK.

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::data::toll_fee_list_cache::get_toll_fee_list_context;
use crate::configs::config::Config;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::configs::rating_db::RATING_DB;
use crate::constants::{bect, checkin, fe};
use crate::db::repositories::account_repository::get_account_for_charge;
use crate::db::repositories::subscriber_repository::{get_subscriber_by_etag, SubscriberInfo};
use crate::db::repositories::TransportTransactionStage;
use crate::fe_protocol;
use crate::models::TCOCmessages::{FE_CHECKIN, FE_CHECKIN_IN_RESP};
use crate::models::TollCache::{TOLL, TOLL_LANE};
use crate::models::ETDR::{save_etdr, ETDR};
use crate::services::service::Service;
use crate::services::transport_transaction_stage_service::TransportTransactionStageService;
use aes;
use cbc;
use r2d2::Pool;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

use super::common::{
    get_existing_pending_ticket_id_for_ticket_in_id_bect,
    normalize_station_type,
    serialize_and_encrypt_response, ticket_type_from_query_only_for_checkin_resp,
    PLATE_EMPTY_SENTINEL,
};
use crate::utils::{bect_skip_account_check, normalize_etag};

/// Xử lý CHECKIN (process): tính phí, lưu DB, trả FE_CHECKIN_IN_RESP.
pub(crate) async fn process_checkin(
    fe_checkin: &FE_CHECKIN,
    conn_id: i32,
    toll: &TOLL,
    toll_lane: &TOLL_LANE,
    encryptor: &cbc::Encryptor<aes::Aes128>,
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
) -> Result<(Vec<u8>, i32), Box<dyn Error>> {
    let lane_type = &toll_lane.lane_type;
    tracing::debug!(conn_id, request_id = fe_checkin.request_id, lane_type = %lane_type, etag = %fe_checkin.etag.trim(), "[CHECKIN] CHECKIN handler entered");
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
        "[CHECKIN] CHECKIN external call finished"
    );

    if let Some(ref _sub) = subscriber_info {
        tracing::debug!(
            request_id = fe_checkin.request_id,
            "[CHECKIN] subscriber found"
        );
    } else {
        tracing::error!(conn_id, request_id = fe_checkin.request_id, etag = %etag_query, "[CHECKIN] CHECKIN subscriber info not found");
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

    // IN only: min balance check then create ETDR / process check-in. OUT is handled by checkout_reserve.
    // Kiểm tra số dư tối thiểu khi check-in lane IN.
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
                            "[CHECKIN] CHECKIN failed"
                        );
                        let reply_bytes =
                            serialize_and_encrypt_response(&fe_resp, encryptor).await?;
                        return Ok((reply_bytes, fe_resp.status));
                    }
                }
            }
        }

        tracing::debug!(request_id = fe_checkin.request_id, etag = %fe_checkin.etag, "[CHECKIN] IN lane create ETDR");

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
                "[CHECKIN] CHECKIN IN idempotent: reuse existing ticket_id for ticket_in_id"
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
            "[CHECKIN] Stage built, saving to DB"
        );
        let transport_service = TransportTransactionStageService::default();
        match transport_service.save(&transport_stage).await {
            Ok(transport_trans_id) => {
                tracing::debug!(
                    request_id = fe_checkin.request_id,
                    transport_trans_id,
                    "[CHECKIN] Stage saved"
                );

                // save() đã verify trong DB và ghi cache → get_by_id một lần (thường hit cache).
                let verified_record_opt = match transport_service
                    .get_by_id(transport_trans_id)
                    .await
                {
                    Ok(Some(r)) => Some(r),
                    Ok(None) => {
                        tracing::warn!(conn_id, request_id = fe_checkin.request_id, transport_trans_id, etag = %fe_checkin.etag.trim(), "[CHECKIN] CHECKIN TRANSPORT_TRANSACTION_STAGE not found after save (cache/DB miss)");
                        None
                    }
                    Err(e) => {
                        tracing::warn!(conn_id, request_id = fe_checkin.request_id, transport_trans_id, error = %e, "[CHECKIN] CHECKIN get_by_id after save failed");
                        None
                    }
                };

                match verified_record_opt {
                    Some(_db_record) => {
                        // ETDR cache làm chuẩn: cập nhật cache từ etdr đã có (từ checkin), không ghi đè từ DB.
                        etdr.db_saved = true;
                        save_etdr(etdr.clone());
                        etdr_for_resp = etdr.clone();
                        tracing::debug!(request_id = fe_checkin.request_id, transport_trans_id, ticket_id = ticket_id_from_sequence, etag = %fe_checkin.etag, "[CHECKIN] ETDR cache updated (source of truth from checkin, DB verified)");
                    }
                    None => {
                        tracing::warn!(conn_id, request_id = fe_checkin.request_id, transport_trans_id, etag = %fe_checkin.etag.trim(), "[CHECKIN] CHECKIN TRANSPORT_TRANSACTION_STAGE not found after save and verify retries");
                        // Luồng lỗi: ghi lại status = "0" vào DB
                        let mut stage_status_zero = transport_stage.clone();
                        stage_status_zero.status = Some("0".to_string());
                        if let Err(e) = transport_service
                            .update(transport_trans_id, &stage_status_zero)
                            .await
                        {
                            tracing::warn!(conn_id, request_id = fe_checkin.request_id, transport_trans_id, error = %e, "[CHECKIN] CHECKIN update STATUS to 0 after verify fail failed");
                        }
                        etdr.check = fe::VERIFY_DB_FAILED;
                        etdr.db_saved = false;
                        etdr.db_save_retry_count += 1;
                        save_etdr(etdr.clone());
                    }
                }
            }
            Err(e) => {
                tracing::error!(conn_id, request_id = fe_checkin.request_id, etag = %fe_checkin.etag.trim(), error = %e, "[CHECKIN] CHECKIN save TRANSPORT_TRANSACTION_STAGE failed");
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

    tracing::debug!(
        request_id = fe_checkin.request_id,
        status = fe_resp.status,
        "[CHECKIN] resp"
    );
    let step_serialize = Instant::now();
    let reply_bytes = serialize_and_encrypt_response(&fe_resp, encryptor).await?;
    let elapsed_ms = step_serialize.elapsed().as_millis() as u64;
    tracing::debug!(
        conn_id,
        request_id = fe_checkin.request_id,
        elapsed_ms,
        step = "serialize_and_encrypt_response",
        "[CHECKIN] CHECKIN step finished"
    );
    tracing::debug!(
        request_id = fe_checkin.request_id,
        reply_len = reply_bytes.len(),
        "[CHECKIN] reply"
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
