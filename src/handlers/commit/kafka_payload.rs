//! Helper build payload checkin/checkout từ ETDR và gửi Kafka (producer hoặc pending).

use crate::models::ETDR::ETDR;
use crate::services::{
    get_kafka_producer, push_pending_checkin, push_pending_checkout, CheckinCommitPayload,
    CheckoutCommitPayload,
};
use crate::utils::normalize_etag;

/// Override khi build checkin payload từ ETDR (None = lấy từ etdr).
#[derive(Default)]
pub struct CheckinPayloadOverrides {
    pub request_id: Option<i64>,
    pub tid: Option<String>,
    pub ticket_in_id: Option<i64>,
    pub etag: Option<String>,
    pub plate: Option<String>,
    pub station_in: Option<i32>,
    pub lane_in: Option<i32>,
    pub checkin_commit_datetime: Option<i64>,
}

/// Build CheckinCommitPayload từ ETDR và overrides.
pub fn build_checkin_payload_from_etdr(
    etdr: &ETDR,
    overrides: CheckinPayloadOverrides,
) -> CheckinCommitPayload {
    let raw_tid = overrides.tid.clone().unwrap_or_else(|| etdr.t_id.clone());
    let tid = normalize_etag(&raw_tid);
    CheckinCommitPayload {
        request_id: overrides.request_id.unwrap_or(etdr.request_id),
        tid,
        ticket_in_id: overrides.ticket_in_id.unwrap_or(etdr.ticket_id),
        ticket_eTag_id: etdr.ticket_eTag_id.unwrap_or(etdr.ref_trans_id),
        etag: overrides
            .etag
            .clone()
            .unwrap_or_else(|| etdr.etag_number.clone()),
        plate: overrides
            .plate
            .clone()
            .unwrap_or_else(|| etdr.plate.clone()),
        station_in: overrides.station_in.unwrap_or(etdr.station_id),
        lane_in: overrides.lane_in.unwrap_or(etdr.lane_id),
        checkin_datetime: etdr.checkin_datetime,
        checkin_commit_datetime: overrides
            .checkin_commit_datetime
            .unwrap_or(etdr.checkin_commit_datetime),
        status: etdr.status,
        register_vehicle_type: etdr.register_vehicle_type.clone(),
        seat: etdr.seat_num.unwrap_or(0),
        weight_goods: etdr.weight_goods.unwrap_or(0),
        weight_all: etdr.weight_all.unwrap_or(0),
        // Chuyển vehicle_type (String trong ETDR) sang i32 cho Kafka producer.
        vehicle_type: etdr.vehicle_type.parse().unwrap_or(1),
    }
}

/// Override khi build checkout payload từ ETDR (None = lấy từ etdr).
#[derive(Default)]
pub struct CheckoutPayloadOverrides {
    pub request_id: Option<i64>,
    pub tid: Option<String>,
    pub ticket_in_id: Option<i64>,
    pub ticket_out_id: Option<i64>,
    pub hub_id: Option<i64>,
    pub etag: Option<String>,
    pub plate: Option<String>,
    pub station_in: Option<i32>,
    pub lane_in: Option<i32>,
    pub checkin_datetime: Option<i64>,
    pub checkin_commit_datetime: Option<i64>,
    pub station_out: Option<i32>,
    pub lane_out: Option<i32>,
    pub checkout_datetime: Option<i64>,
    pub checkout_commit_datetime: Option<i64>,
    pub register_vehicle_type: Option<String>,
    pub seat: Option<i32>,
    pub weight_goods: Option<i32>,
    pub weight_all: Option<i32>,
    pub vehicle_type: Option<i32>,
    pub trans_amount: Option<i32>,
    pub status: Option<i32>,
    pub rating_detail: Option<Vec<crate::models::ETDR::BOORatingDetail>>,
}

/// Build CheckoutCommitPayload từ ETDR và overrides.
pub fn build_checkout_payload_from_etdr(
    etdr: &ETDR,
    overrides: CheckoutPayloadOverrides,
) -> CheckoutCommitPayload {
    let raw_tid = overrides.tid.clone().unwrap_or_else(|| etdr.t_id.clone());
    let tid = normalize_etag(&raw_tid);
    CheckoutCommitPayload {
        request_id: overrides.request_id.unwrap_or(etdr.request_id),
        tid,
        ticket_in_id: overrides.ticket_in_id.unwrap_or(etdr.ticket_id),
        ticket_eTag_id: etdr.ticket_eTag_id.unwrap_or(etdr.ref_trans_id),
        hub_id: overrides.hub_id.or(etdr.hub_id),
        ticket_out_id: overrides.ticket_out_id.unwrap_or(etdr.ticket_id),
        etag: overrides
            .etag
            .clone()
            .unwrap_or_else(|| etdr.etag_number.clone()),
        plate: overrides
            .plate
            .clone()
            .unwrap_or_else(|| etdr.plate.clone()),
        station_in: overrides.station_in.unwrap_or(etdr.station_id),
        lane_in: overrides.lane_in.unwrap_or(etdr.lane_id),
        checkin_datetime: overrides.checkin_datetime.unwrap_or(etdr.checkin_datetime),
        checkin_commit_datetime: overrides
            .checkin_commit_datetime
            .unwrap_or(etdr.checkin_commit_datetime),
        station_out: overrides.station_out.unwrap_or(etdr.toll_out),
        lane_out: overrides.lane_out.unwrap_or(etdr.lane_out),
        checkout_datetime: overrides
            .checkout_datetime
            .unwrap_or(etdr.checkout_datetime),
        checkout_commit_datetime: overrides
            .checkout_commit_datetime
            .unwrap_or(etdr.checkout_commit_datetime),
        register_vehicle_type: overrides
            .register_vehicle_type
            .clone()
            .unwrap_or_else(|| etdr.register_vehicle_type.clone()),
        seat: overrides.seat.unwrap_or_else(|| etdr.seat_num.unwrap_or(0)),
        weight_goods: overrides
            .weight_goods
            .unwrap_or_else(|| etdr.weight_goods.unwrap_or(0)),
        weight_all: overrides
            .weight_all
            .unwrap_or_else(|| etdr.weight_all.unwrap_or(0)),
        // Chuyển vehicle_type (String trong ETDR hoặc override Option<i32>) sang i32 cho Kafka producer.
        vehicle_type: overrides
            .vehicle_type
            .unwrap_or_else(|| etdr.vehicle_type.parse().unwrap_or(1)),
        ticket_type: "L".to_string(),
        price_ticket_type: 1,
        status: overrides.status.unwrap_or(etdr.status),
        // Bản tin checkout: nếu trans_amount null (None) thì set về 0.
        trans_amount: overrides.trans_amount.unwrap_or(0),
        rating_detail: overrides
            .rating_detail
            .unwrap_or_else(|| etdr.rating_details.clone()),
    }
}

/// Gửi checkin payload lên Kafka (producer nếu ready, ngược lại pending).
#[inline]
pub fn send_checkin_to_kafka(payload: CheckinCommitPayload) {
    if let Some(kafka) = get_kafka_producer() {
        kafka.send_checkin_from_commit_background(payload);
    } else {
        push_pending_checkin(payload);
    }
}

/// Gửi checkout payload lên Kafka (producer nếu ready, ngược lại pending).
#[inline]
pub fn send_checkout_to_kafka(payload: CheckoutCommitPayload) {
    if let Some(kafka) = get_kafka_producer() {
        kafka.send_checkout_from_commit_background(payload);
    } else {
        push_pending_checkout(payload);
    }
}
