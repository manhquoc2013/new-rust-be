//! Gộp dữ liệu ETDR vào bản ghi TRANSPORT_TRANSACTION_STAGE đã có (BECT only; giữ giá trị DB, điền field thiếu từ ETDR).

use crate::configs::config::Config;
use crate::handlers::checkin::common::resolve_toll_type_for_etdr;
use crate::utils::now_utc_db_string;

use crate::models::ETDR::ETDR;

/// Gộp dữ liệu ETDR vào bản ghi TRANSPORT_TRANSACTION_STAGE đã có: giữ giá trị DB, chỉ điền field thiếu từ ETDR.
pub(super) fn merge_etdr_into_transport_stage(
    existing: &crate::db::repositories::TransportTransactionStage,
    etdr: &ETDR,
) -> crate::db::repositories::TransportTransactionStage {
    use crate::db::repositories::TransportTransactionStage;
    let from_etdr =
        |v: Option<i64>, etdr_val: i64| v.or(if etdr_val != 0 { Some(etdr_val) } else { None });
    let from_etdr_str = |v: Option<String>, etdr_val: &str| {
        v.or_else(|| {
            if etdr_val.is_empty() {
                None
            } else {
                Some(etdr_val.to_string())
            }
        })
    };
    let now = now_utc_db_string();
    TransportTransactionStage {
        subscriber_id: existing.subscriber_id.or(if etdr.sub_id > 0 {
            Some(etdr.sub_id as i64)
        } else {
            None
        }),
        etag_id: existing.etag_id.or(etdr.etag_key.map(|k| k as i64)),
        vehicle_id: from_etdr(existing.vehicle_id, etdr.vehicle_id as i64),
        checkin_toll_id: existing.checkin_toll_id.or(Some(etdr.station_id as i64)),
        checkin_lane_id: existing.checkin_lane_id.or(Some(etdr.lane_id as i64)),
        checkin_commit_datetime: existing.checkin_commit_datetime.clone(),
        checkin_channel: existing.checkin_channel.or(Some(etdr.lane_id as i64)),
        checkin_pass: existing
            .checkin_pass
            .clone()
            .or_else(|| Some("P".to_string())),
        checkin_pass_reason_id: existing.checkin_pass_reason_id.clone(),
        checkout_toll_id: existing.checkout_toll_id.or(if etdr.toll_out != 0 {
            Some(etdr.toll_out as i64)
        } else {
            None
        }),
        checkout_lane_id: existing.checkout_lane_id.or(if etdr.lane_out != 0 {
            Some(etdr.lane_out as i64)
        } else {
            None
        }),
        checkout_commit_datetime: existing.checkout_commit_datetime.clone(),
        checkout_channel: existing.checkout_channel,
        checkout_pass: existing.checkout_pass.clone(),
        checkout_pass_reason_id: existing.checkout_pass_reason_id.clone(),
        charge_status: existing.charge_status.clone(),
        charge_type: Some("L".to_string()),
        total_amount: existing.total_amount.or(if etdr.price != 0 {
            Some(etdr.price as i64)
        } else {
            None
        }),
        account_id: from_etdr(existing.account_id, etdr.account_id as i64),
        account_trans_id: existing.account_trans_id,
        checkin_datetime: existing.checkin_datetime.clone().or_else(|| {
            if etdr.checkin_datetime > 0 {
                crate::utils::epoch_to_datetime_utc(etdr.checkin_datetime)
                    .map(|dt| crate::utils::format_datetime_utc_db(&dt))
            } else {
                None
            }
        }),
        checkout_datetime: existing.checkout_datetime.clone().or_else(|| {
            if etdr.checkout_datetime > 0 {
                crate::utils::epoch_to_datetime_utc(etdr.checkout_datetime)
                    .map(|dt| crate::utils::format_datetime_utc_db(&dt))
            } else {
                None
            }
        }),
        checkin_status: existing
            .checkin_status
            .clone()
            .or_else(|| Some("1".to_string())),
        etag_number: from_etdr_str(existing.etag_number.clone(), &etdr.etag_number),
        request_id: existing.request_id.or(Some(etdr.request_id)),
        checkin_plate: from_etdr_str(existing.checkin_plate.clone(), &etdr.plate),
        checkin_plate_status: existing
            .checkin_plate_status
            .clone()
            .or_else(|| Some(etdr.plate_status.to_string())),
        checkout_plate: existing.checkout_plate.clone().or_else(|| {
            if etdr.plate.is_empty() {
                None
            } else {
                Some(etdr.plate.clone())
            }
        }),
        checkout_plate_status: existing.checkout_plate_status.clone(),
        plate_from_toll: existing.plate_from_toll.clone().or_else(|| {
            if etdr.plate.is_empty() {
                None
            } else {
                Some(etdr.plate.clone())
            }
        }),
        img_count: existing.img_count.or(Some(etdr.image_count)),
        checkin_img_count: existing.checkin_img_count.or(Some(etdr.image_count)),
        checkout_img_count: existing.checkout_img_count,
        status: existing.status.clone().or_else(|| {
            Some(if etdr.status == 0 {
                "1".to_string()
            } else {
                "0".to_string()
            })
        }), // DB: 1 = success, 0 = error; ETDR: 0 = success, non-zero = error
        last_update: Some(now),
        notif_scan: existing.notif_scan,
        toll_type: existing.toll_type.clone().or_else(|| {
            Some(resolve_toll_type_for_etdr(
                etdr.boo_toll_type.as_ref(),
                existing.checkin_toll_id.or(Some(etdr.station_id as i64)),
                existing.checkout_toll_id.or(if etdr.toll_out != 0 {
                    Some(etdr.toll_out as i64)
                } else {
                    None
                }),
            ))
        }),
        rating_type: existing.rating_type.clone().or(etdr.rating_type.clone()),
        insert_datetime: existing.insert_datetime.clone(),
        insuff_amount: existing.insuff_amount,
        checkout_status: existing.checkout_status.clone(),
        bitwise_scan: existing.bitwise_scan,
        plate: from_etdr_str(existing.plate.clone(), &etdr.plate),
        vehicle_type: from_etdr_str(existing.vehicle_type.clone(), &etdr.vehicle_type),
        fe_vehicle_length: existing.fe_vehicle_length,
        fe_commit_amount: existing
            .fe_commit_amount
            .or(Some(etdr.commit_amount as i64)),
        fe_weight: from_etdr(existing.fe_weight, etdr.weight as i64),
        fe_reason_id: existing.fe_reason_id.or(if etdr.reason_id > 0 {
            Some(etdr.reason_id as i64)
        } else {
            None
        }),
        vehicle_type_profile: existing
            .vehicle_type_profile
            .clone()
            .or_else(|| Some(etdr.vehicle_type_profile.clone())),
        boo: existing
            .boo
            .or_else(|| Some(Config::get_vehicle_boo_type(&etdr.etag_number) as i64)),
        boo_transport_trans_id: existing.boo_transport_trans_id.or(Some(etdr.ticket_id)),
        subscription_ids: existing.subscription_ids.clone(),
        checkin_shift: existing.checkin_shift.clone(),
        checkout_shift: existing.checkout_shift.clone(),
        turning_code: existing
            .turning_code
            .clone()
            .or(etdr.toll_turning_time_code.clone()),
        checkin_tid: existing
            .checkin_tid
            .clone()
            .or_else(|| Some(etdr.t_id.clone())),
        checkout_tid: existing.checkout_tid.clone(),
        charge_in: existing.charge_in.clone().or(etdr.charge_in.clone()),
        charge_trans_id: existing.charge_trans_id.clone(),
        balance: existing.balance,
        charge_in_status: existing
            .charge_in_status
            .clone()
            .or(etdr.charge_in_status.clone()),
        charge_datetime: existing.charge_datetime.clone(),
        charge_in_104: existing.charge_in_104.clone().or(etdr.charge_in104.clone()),
        fe_online_status: existing.fe_online_status.clone(),
        mdh_id: existing.mdh_id,
        chd_type: existing.chd_type.clone().or(etdr.chd_type.clone()),
        chd_ref_id: existing
            .chd_ref_id
            .or(etdr.chd_ref_id.as_ref().and_then(|s| s.parse::<i64>().ok())),
        chd_reason: existing.chd_reason.clone().or(etdr.chd_reason.clone()),
        fe_trans_id: existing.fe_trans_id.clone().or(etdr.fe_trans_id.clone()),
        transition_close: existing
            .transition_close
            .clone()
            .or_else(|| Some("F".to_string())),
        voucher_code: existing.voucher_code.clone(),
        voucher_used_amount: existing.voucher_used_amount,
        voucher_amount: existing.voucher_amount,
        transport_sync_id: existing.transport_sync_id,
        ticket_in_id: existing.ticket_in_id.or(etdr.ticket_in_id),
        hub_id: existing.hub_id.or(etdr.hub_id),
        ticket_eTag_id: existing.ticket_eTag_id.or(etdr.ticket_eTag_id),
        ticket_out_id: existing.ticket_out_id.or(etdr.ticket_out_id),
        token_id: existing.token_id.clone(),
        acs_account_no: existing.acs_account_no.clone(),
        boo_etag: existing.boo_etag.clone(),
        sub_charge_in: existing
            .sub_charge_in
            .clone()
            .or(etdr.sub_charge_in.clone()),
        sync_status: existing.sync_status.or({
            if etdr.sync_status == 0 || etdr.sync_status == 1 {
                Some(etdr.sync_status as i64)
            } else {
                existing.sync_status
            }
        }),
        ..*existing
    }
}

/// Merge two ETDRs with the same ticket_id into one for retry: take the more complete record (later timestamps, non-empty fields from either).
/// Used so retry store has at most one record per ticket_id with merged data.
pub fn merge_etdr_same_ticket_id(a: &ETDR, b: &ETDR) -> ETDR {
    let (base, other) = if a.checkin_datetime >= b.checkin_datetime {
        (a.clone(), b)
    } else {
        (b.clone(), a)
    };
    let take_i64 = |v: i64, w: i64| if w != 0 { w } else { v };
    let take_i32 = |v: i32, w: i32| if w != 0 { w } else { v };
    let take_str = |v: &str, w: &str| {
        if w.is_empty() {
            v.to_string()
        } else {
            w.to_string()
        }
    };
    ETDR {
        checkin_datetime: std::cmp::max(base.checkin_datetime, other.checkin_datetime),
        checkin_commit_datetime: std::cmp::max(
            base.checkin_commit_datetime,
            other.checkin_commit_datetime,
        ),
        checkout_datetime: take_i64(base.checkout_datetime, other.checkout_datetime),
        checkout_commit_datetime: take_i64(
            base.checkout_commit_datetime,
            other.checkout_commit_datetime,
        ),
        station_id: if base.station_id != 0 {
            base.station_id
        } else {
            other.station_id
        },
        lane_id: if base.lane_id != 0 {
            base.lane_id
        } else {
            other.lane_id
        },
        toll_out: take_i32(base.toll_out, other.toll_out),
        lane_out: take_i32(base.lane_out, other.lane_out),
        plate: take_str(&base.plate, &other.plate),
        price: take_i32(base.price, other.price),
        commit_amount: take_i32(base.commit_amount, other.commit_amount),
        image_count: if base.image_count != 0 {
            base.image_count
        } else {
            other.image_count
        },
        etag_number: take_str(&base.etag_number, &other.etag_number),
        db_save_retry_count: std::cmp::max(base.db_save_retry_count, other.db_save_retry_count),
        ..base
    }
}
