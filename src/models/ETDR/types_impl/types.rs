//! Định nghĩa kiểu ETDR và các struct liên quan.

use std::fmt;

use serde::{Deserialize, Serialize};

/// TTL ETDR: re-export từ constants (1 giờ từ thời điểm checkout).
#[allow(unused_imports)]
pub use crate::constants::etdr::ETDR_TTL_MS;

/// Chi tiết tính phí trạm kín
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct TransStageDetail {
    // TODO: Thêm các field chi tiết khi có thông tin cụ thể
}

/// Chi tiết tính phí tuyến liên thông (dùng chung cho ETDR và Kafka Hub).
/// Có đủ price_id, bot_id, stage_id để lưu BOO_TRANS_STAGE_TCD / TRANSPORT_TRANS_STAGE_TCD khi retry.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BOORatingDetail {
    pub boo: i32,
    pub toll_a_id: i32,
    pub toll_b_id: i32,
    pub ticket_type: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub subscription_id: Option<String>,
    pub price_ticket_type: i32,
    pub price_amount: i32,
    pub vehicle_type: i32,
    /// Chỉ dùng nội bộ (DB/TCD); không gửi lên Kafka.
    #[serde(skip_serializing, default)]
    pub price_id: Option<i64>,
    #[serde(skip_serializing, default)]
    pub bot_id: Option<i64>,
    #[serde(skip_serializing, default)]
    pub stage_id: Option<i64>,
}

/// Loại giao dịch trạm kín
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct TransitionClose {
    // TODO: Thêm các field chi tiết khi có thông tin cụ thể
}

/// Thông tin rollback
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct InfoRollBack {
    // TODO: Thêm các field chi tiết khi có thông tin cụ thể
}

/// Event object (tương thích HUB).
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    // TODO: Thêm các field chi tiết khi có thông tin cụ thể
}

/// ETDR (Entry Transaction Data Record) — lưu trữ thông tin giao dịch checkin làn vào.
/// TTL chỉ áp dụng khi đã checkout (1h từ checkout); chưa checkout không TTL. Truy vấn tìm trong cache memory và KeyDB.
#[allow(clippy::upper_case_acronyms)]
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct ETDR {
    // 1. Thông tin cơ bản (Basic Information)
    pub time_update: i64,
    pub sub_id: i32,
    pub etag_id: String,
    pub etag_key: Option<i32>,
    pub etag_number: String,
    pub account_id: i32,
    pub vehicle_id: i32,
    /// Trạng thái BOO: 0 = thành công, khác 0 = lỗi (thống nhất với response BOO và Kafka payload).
    pub status: i32,
    pub hash_value: String,

    // 2. Thông tin phương tiện (Vehicle Information)
    pub vehicle_type: String,
    pub vehicle_type_ext1: String,
    pub vehicle_type_ext2: String,
    pub vehicle_type_ext3: String,
    pub vehicle_type_ext4: String,
    pub vehicle_type_ext5: String,
    pub vehicle_type_ext6: String,
    pub vehicle_type_ext7: String,
    pub vehicle_type_ext8: String,
    pub vehicle_type_ext9: String,
    pub vehicle_type_ext10: String,
    pub vehicle_type_profile: String,
    pub plate: String,
    pub plate_status: i32,
    pub seat_num: Option<i32>,
    pub weight: i32,
    pub weight_goods: Option<i32>,
    pub weight_all: Option<i32>,
    pub register_vehicle_type: String,
    pub vehicle_length: i32,

    // 3. Thông tin thuê bao và thẻ (Subscriber & Tag Information)
    pub sub_status: String,
    pub etag_status: String,
    pub auto_extend: String,
    pub price_type: String,
    pub start_date: String,

    // 4. Thông tin giao dịch (Transaction Information)
    pub request_id: i64,
    /// Định danh duy nhất của giao dịch (transport_trans_id); luôn có khi tạo ETDR từ checkin; dùng để tra cứu đúng giao dịch.
    pub ticket_id: i64,
    pub ref_trans_id: i64,
    pub command_id: i32,
    pub t_id: String,
    pub ticket_type: i32,
    pub price_ticket_type: i32,
    pub price: i32,
    pub price_id: Option<i32>,
    pub commit_amount: i32,
    pub reason_id: i32,
    pub image_count: i32,

    // 5. Thông tin trạm và làn (Station & Lane Information)
    pub station_id: i32,
    pub station_type: i32,
    pub lane_id: i32,
    pub toll_out: i32,
    pub lane_out: i32,
    pub ref_station_id: Option<i32>,
    pub ref_lane_id: Option<i32>,

    // 6. Thông tin thời gian (Time Information)
    pub time_route_checkin: i64,
    pub time_route_checkin_commit: i64,
    pub time_route_checkout: i64,
    pub time_station_checkin: i64,
    pub checkin_datetime: i64,
    pub checkin_commit_datetime: i64,
    pub checkout_datetime: i64,
    pub checkout_commit_datetime: i64,

    // 7. Thông tin BOO (BOO Information)
    pub boo_ticket_id: i64,
    pub boo_trans_amount: i32,
    pub boo_trans_datetime: Option<i64>,
    pub boo_lane_type: Option<String>,
    pub boo_toll_type: Option<String>,
    pub boo_ticket_type: Option<String>,
    pub boo_subscription_ids: Option<String>,

    // 8. Thông tin thanh toán (Payment Information)
    pub charge_in: Option<String>,
    pub sub_charge_in: Option<String>,
    pub charge_in_status: Option<String>,
    pub charge_trans_id: Option<String>,
    pub charge_datetime: Option<i64>,
    pub charge_in104: Option<String>,

    // 9. Thông tin giao dịch kép (Dual Transaction)
    pub etdr_dual: Option<Box<ETDR>>,
    pub chd_type: Option<String>,
    pub chd_ref_id: Option<String>,
    pub chd_reason: Option<String>,

    // 10. Thông tin trạm kín (Closed Station Information)
    pub trans_stage_details: Vec<TransStageDetail>,
    pub rating_details: Vec<BOORatingDetail>,
    pub transition_close: Option<TransitionClose>,
    pub toll_turning_time_code: Option<String>,

    // 11. Thông tin khác (Other Information)
    pub check: i32,
    pub log_request: Option<String>,
    pub info_roll_back: Option<InfoRollBack>,
    pub free_turning: bool,
    pub enough_min_balance: bool,
    pub rating_type: Option<String>,
    pub fe_trans_id: Option<String>,
    pub ticket_in_id: Option<i64>,
    pub ticket_eTag_id: Option<i64>,
    pub ticket_out_id: Option<i64>,
    pub hub_id: Option<i64>,
    #[serde(default)]
    pub checkin_from_sync: bool,
    pub event: Option<Event>,

    // 12. Thông tin lưu trữ DB (Database Persistence Information)
    pub db_saved: bool,
    pub db_save_retry_count: i32,
    /// 0 = not synced, 1 = synced. Khi commit giao dịch set về 0 (not synced).
    #[serde(default)]
    pub sync_status: i32,
}

impl fmt::Debug for ETDR {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ETDR")
            .field("time_update", &self.time_update)
            .field("sub_id", &self.sub_id)
            .field("etag_id", &self.etag_id)
            .field("etag_key", &self.etag_key)
            .field("etag_number", &self.etag_number)
            .field("account_id", &self.account_id)
            .field("vehicle_id", &self.vehicle_id)
            .field("status", &self.status)
            .field("hash_value", &self.hash_value)
            .field("vehicle_type", &self.vehicle_type)
            .field("vehicle_type_profile", &self.vehicle_type_profile)
            .field("plate", &self.plate)
            .field("plate_status", &self.plate_status)
            .field("seat_num", &self.seat_num)
            .field("weight", &self.weight)
            .field("weight_goods", &self.weight_goods)
            .field("weight_all", &self.weight_all)
            .field("register_vehicle_type", &self.register_vehicle_type)
            .field("vehicle_length", &self.vehicle_length)
            .field("sub_status", &self.sub_status)
            .field("etag_status", &self.etag_status)
            .field("auto_extend", &self.auto_extend)
            .field("price_type", &self.price_type)
            .field("start_date", &self.start_date)
            .field("request_id", &self.request_id)
            .field("ticket_id", &self.ticket_id)
            .field("ref_trans_id", &self.ref_trans_id)
            .field("command_id", &self.command_id)
            .field("t_id", &self.t_id)
            .field("ticket_type", &self.ticket_type)
            .field("price_ticket_type", &self.price_ticket_type)
            .field("price", &self.price)
            .field("price_id", &self.price_id)
            .field("commit_amount", &self.commit_amount)
            .field("reason_id", &self.reason_id)
            .field("image_count", &self.image_count)
            .field("station_id", &self.station_id)
            .field("station_type", &self.station_type)
            .field("lane_id", &self.lane_id)
            .field("toll_out", &self.toll_out)
            .field("lane_out", &self.lane_out)
            .field("ref_station_id", &self.ref_station_id)
            .field("ref_lane_id", &self.ref_lane_id)
            .field("checkin_datetime", &self.checkin_datetime)
            .field("checkin_commit_datetime", &self.checkin_commit_datetime)
            .field("checkout_datetime", &self.checkout_datetime)
            .field("boo_ticket_id", &self.boo_ticket_id)
            .field("boo_trans_amount", &self.boo_trans_amount)
            .field("boo_trans_datetime", &self.boo_trans_datetime)
            .field("boo_lane_type", &self.boo_lane_type)
            .field("boo_toll_type", &self.boo_toll_type)
            .field("boo_ticket_type", &self.boo_ticket_type)
            .field("boo_subscription_ids", &self.boo_subscription_ids)
            .field("charge_in", &self.charge_in)
            .field("sub_charge_in", &self.sub_charge_in)
            .field("charge_in_status", &self.charge_in_status)
            .field("charge_trans_id", &self.charge_trans_id)
            .field("charge_datetime", &self.charge_datetime)
            .field("charge_in104", &self.charge_in104)
            .field("check", &self.check)
            .field("free_turning", &self.free_turning)
            .field("enough_min_balance", &self.enough_min_balance)
            .field("rating_type", &self.rating_type)
            .field("fe_trans_id", &self.fe_trans_id)
            .finish()
    }
}
