//! Cấu trúc sự kiện gửi Kafka (checkin, checkout), CheckinEventData, CheckoutEventData.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::models::ETDR::BOORatingDetail;

/// Parse Option<i32> từ null, missing, number (i32/float) hoặc string (consumer HUB).
/// Hỗ trợ HUB gửi status kiểu str (vd. "0", "1") — convert sang i32; number nguyên/float cũng chấp nhận.
fn deserialize_opt_i32_flexible<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<Value>::deserialize(deserializer)?;
    match v {
        None => Ok(None),
        Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_i64()
            .and_then(|x| i32::try_from(x).ok())
            .map(Some)
            .or_else(|| n.as_f64().map(|f| Some(f as i32)))
            .ok_or_else(|| serde::de::Error::custom("invalid i32 number")),
        Some(Value::String(s)) => s
            .trim()
            .parse::<i32>()
            .map(Some)
            .map_err(serde::de::Error::custom),
        _ => Err(serde::de::Error::custom("expected null, number or string")),
    }
}

/// Data payload cho CHECKIN_INFO
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CheckinEventData {
    pub request_id: i64,
    pub tid: String,
    pub ticket_in_id: i64,
    #[serde(rename = "ticket_eTag_id")]
    pub ticket_eTag_id: i64,
    #[serde(rename = "eTag")]
    pub etag: String,
    pub plate: String,
    pub station_in: i32,
    pub lane_in: i32,
    pub checkin_datetime: i64,
    pub checkin_commit_datetime: i64,
    /// Trạng thái (Kafka message: i32, serialize ra JSON number).
    pub status: i32,
    pub register_vehicle_type: String,
    pub seat: i32,
    pub weight_goods: i32,
    pub weight_all: i32,
    /// Loại phương tiện (Kafka message: i32, serialize ra JSON number).
    pub vehicle_type: i32,
}

/// Event envelope cho CHECKIN_INFO
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CheckinEvent {
    pub event_id: String,
    pub event_type: String, // "CHECKIN_INFO"
    pub timestamp: i64,
    pub trace_id: String,
    pub data_version: String,
    pub data: CheckinEventData,
}

/// Data payload cho CHECKOUT_INFO
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CheckoutEventData {
    pub request_id: i64,
    pub tid: String,
    pub ticket_in_id: i64,
    #[serde(rename = "ticket_eTag_id")]
    pub ticket_eTag_id: i64,
    /// Mã hub (từ Kafka CHECKIN_HUB_INFO hoặc response checkout); luôn đưa vào message CHECKOUT_INFO.
    pub hub_id: Option<i64>,
    pub ticket_out_id: i64,
    #[serde(rename = "eTag")]
    pub etag: String,
    pub plate: String,
    pub station_in: i32,
    pub lane_in: i32,
    pub checkin_datetime: i64,
    pub checkin_commit_datetime: i64,
    pub station_out: i32,
    pub lane_out: i32,
    pub checkout_datetime: i64,
    pub checkout_commit_datetime: i64,
    pub register_vehicle_type: String,
    pub seat: i32,
    pub weight_goods: i32,
    pub weight_all: i32,
    /// Loại phương tiện (Kafka message: i32, serialize ra JSON number).
    pub vehicle_type: i32,
    pub ticket_type: String,
    pub price_ticket_type: i32,
    /// Trạng thái (Kafka message: i32, serialize ra JSON number).
    pub status: i32,
    pub trans_amount: i32,
    pub rating_detail_line: i32,
    pub rating_detail: Vec<BOORatingDetail>,
}

/// Event envelope cho CHECKOUT_INFO
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CheckoutEvent {
    pub event_id: String,
    pub event_type: String, // "CHECKOUT_INFO"
    pub timestamp: i64,
    pub trace_id: String,
    pub data_version: String,
    pub data: CheckoutEventData,
}

// --- Consumer: bản tin đồng bộ checkin từ HUB về BOO (topic topics.hub.checkin.trans.online) ---

/// Data payload cho CHECKIN_HUB_INFO (HUB push về BOO).
/// Consumer: tất cả field đều Option để chấp nhận thiếu từ HUB.
/// HUB có thể gửi key snake_case ("request_id") hoặc camelCase ("requestId") — dùng alias để chấp nhận cả hai.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CheckinHubInfoEventData {
    #[serde(alias = "requestId")]
    pub request_id: Option<String>,
    pub tid: Option<String>,
    #[serde(alias = "ticketInId")]
    pub ticket_in_id: Option<i64>,
    #[serde(rename = "ticket_eTag_id", alias = "ticketETagId")]
    pub ticket_eTag_id: Option<i64>,
    #[serde(alias = "hubId")]
    pub hub_id: Option<i64>,
    #[serde(rename = "eTag")]
    pub etag: Option<String>,
    pub plate: Option<String>,
    #[serde(alias = "stationIn")]
    pub station_in: Option<i32>,
    #[serde(alias = "laneIn")]
    pub lane_in: Option<i32>,
    #[serde(alias = "checkinDatetime")]
    pub checkin_datetime: Option<i64>,
    #[serde(alias = "checkinCommitDatetime")]
    pub checkin_commit_datetime: Option<i64>,
    /// Trạng thái từ HUB (i32). Consumer chấp nhận number hoặc str (vd. "0", "1") — convert sang i32.
    #[serde(default, deserialize_with = "deserialize_opt_i32_flexible")]
    pub status: Option<i32>,
    #[serde(alias = "registerVehicleType")]
    pub register_vehicle_type: Option<String>,
    pub seat: Option<i32>,
    #[serde(alias = "weightGoods")]
    pub weight_goods: Option<i32>,
    #[serde(alias = "weightAll")]
    pub weight_all: Option<i32>,
    #[serde(
        default,
        deserialize_with = "deserialize_opt_i32_flexible",
        alias = "vehicleType"
    )]
    pub vehicle_type: Option<i32>,
}

/// Minimal envelope: chỉ có timestamp + data (HUB đôi khi gửi không có event_id/event_type/trace_id/data_version).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CheckinHubInfoEventMinimal {
    pub timestamp: i64,
    pub data: CheckinHubInfoEventData,
}

/// Event envelope cho CHECKIN_HUB_INFO (consumer nhận từ HUB).
/// HUB có thể gửi eventId/eventType/dataVersion (camelCase) — alias để chấp nhận cả hai.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CheckinHubInfoEvent {
    #[serde(alias = "eventId")]
    pub event_id: String,
    #[serde(alias = "eventType")]
    pub event_type: String, // "CHECKIN_HUB_INFO"
    pub timestamp: i64,
    #[serde(alias = "traceId")]
    pub trace_id: String,
    #[serde(alias = "dataVersion")]
    pub data_version: String,
    pub data: CheckinHubInfoEventData,
}
