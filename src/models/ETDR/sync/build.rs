//! Tạo bản ghi TRANSPORT_TRANSACTION_STAGE từ ETDR (BECT only; khi chưa có trong DB).

use crate::configs::config::Config;
use crate::db::repositories::TransportTransactionStage;
use crate::handlers::checkin::common::resolve_toll_type_for_etdr;
use crate::utils::now_utc_db_string;

use crate::models::ETDR::ETDR;

/// Tạo TransportTransactionStage từ ETDR với thông tin tối thiểu (dùng khi retry save BECT, record chưa tồn tại).
pub(super) fn build_transport_stage_from_etdr(etdr: &ETDR) -> TransportTransactionStage {
    let now = now_utc_db_string();
    let checkin_datetime = if etdr.checkin_datetime > 0 {
        Some(
            crate::utils::epoch_to_datetime_utc(etdr.checkin_datetime)
                .map(|dt| crate::utils::format_datetime_utc_db(&dt))
                .unwrap_or(now.clone()),
        )
    } else {
        Some(now.clone())
    };

    TransportTransactionStage {
        transport_trans_id: etdr.ticket_id,
        subscriber_id: if etdr.sub_id > 0 {
            Some(etdr.sub_id as i64)
        } else {
            None
        },
        etag_id: etdr.etag_key.map(|k| k as i64),
        vehicle_id: if etdr.vehicle_id > 0 {
            Some(etdr.vehicle_id as i64)
        } else {
            None
        },
        checkin_toll_id: Some(etdr.station_id as i64),
        checkin_lane_id: Some(etdr.lane_id as i64),
        checkin_commit_datetime: None,
        checkin_channel: Some(etdr.lane_id as i64),
        checkin_pass: Some("P".to_string()),
        checkin_pass_reason_id: None,
        checkout_toll_id: if etdr.toll_out != 0 {
            Some(etdr.toll_out as i64)
        } else {
            None
        },
        checkout_lane_id: if etdr.lane_out != 0 {
            Some(etdr.lane_out as i64)
        } else {
            None
        },
        checkout_commit_datetime: None,
        checkout_channel: None,
        checkout_pass: None,
        checkout_pass_reason_id: None,
        charge_status: None,
        charge_type: None,
        total_amount: None,
        account_id: if etdr.account_id > 0 {
            Some(etdr.account_id as i64)
        } else {
            None
        },
        account_trans_id: None,
        checkin_datetime,
        checkout_datetime: None,
        checkin_status: Some("2".to_string()),
        etag_number: Some(etdr.etag_number.clone()),
        request_id: Some(etdr.request_id),
        checkin_plate: Some(etdr.plate.clone()),
        checkin_plate_status: Some(etdr.plate_status.to_string()),
        checkout_plate: None,
        checkout_plate_status: None,
        plate_from_toll: if !etdr.plate.is_empty() {
            Some(etdr.plate.clone())
        } else {
            None
        },
        img_count: Some(etdr.image_count),
        checkin_img_count: Some(etdr.image_count),
        checkout_img_count: None,
        status: Some(if etdr.status == 0 { "1" } else { "0" }.to_string()), // DB: 1 = success, 0 = error
        last_update: Some(now.clone()),
        notif_scan: None,
        toll_type: Some(resolve_toll_type_for_etdr(
            etdr.boo_toll_type.as_ref(),
            Some(etdr.station_id as i64),
            if etdr.toll_out != 0 {
                Some(etdr.toll_out as i64)
            } else {
                None
            },
        )),
        rating_type: etdr.rating_type.clone(),
        insert_datetime: Some(now.clone()),
        insuff_amount: None,
        checkout_status: None,
        bitwise_scan: None,
        plate: Some(etdr.plate.clone()),
        vehicle_type: Some(etdr.vehicle_type.clone()),
        fe_vehicle_length: None,
        fe_commit_amount: Some(etdr.commit_amount as i64),
        fe_weight: if etdr.weight > 0 {
            Some(etdr.weight as i64)
        } else {
            None
        },
        fe_reason_id: if etdr.reason_id > 0 {
            Some(etdr.reason_id as i64)
        } else {
            None
        },
        vehicle_type_profile: Some(etdr.vehicle_type_profile.clone()),
        boo: Some(Config::get_vehicle_boo_type(&etdr.etag_number) as i64),
        boo_transport_trans_id: Some(etdr.ticket_id),
        subscription_ids: None,
        checkin_shift: None,
        checkout_shift: None,
        turning_code: None,
        checkin_tid: Some(etdr.t_id.clone()),
        checkout_tid: None,
        charge_in: etdr.charge_in.clone(),
        charge_trans_id: None,
        balance: None,
        charge_in_status: etdr.charge_in_status.clone(),
        charge_datetime: None,
        charge_in_104: etdr.charge_in104.clone(),
        fe_online_status: None,
        mdh_id: None,
        chd_type: etdr.chd_type.clone(),
        chd_ref_id: etdr.chd_ref_id.as_ref().and_then(|s| s.parse::<i64>().ok()),
        chd_reason: etdr.chd_reason.clone(),
        fe_trans_id: etdr.fe_trans_id.clone(),
        transition_close: Some("F".to_string()),
        voucher_code: None,
        voucher_used_amount: None,
        voucher_amount: None,
        transport_sync_id: None,
        ticket_in_id: etdr.ticket_in_id,
        hub_id: etdr.hub_id,
        ticket_eTag_id: etdr.ticket_eTag_id,
        ticket_out_id: None,
        token_id: None,
        acs_account_no: None,
        boo_etag: None,
        sub_charge_in: etdr.sub_charge_in.clone(),
        sync_status: None,
    }
}
