//! Payloads and message types for Kafka producer.

use crate::models::ETDR::BOORatingDetail;
use serde::{Deserialize, Serialize};

/// Payload to send checkin event in background (owned data when spawning).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckinCommitPayload {
    pub request_id: i64,
    pub tid: String,
    pub ticket_in_id: i64,
    pub ticket_eTag_id: i64,
    pub etag: String,
    pub plate: String,
    pub station_in: i32,
    pub lane_in: i32,
    pub checkin_datetime: i64,
    pub checkin_commit_datetime: i64,
    /// Status (i32, sent to Kafka as number).
    pub status: i32,
    pub register_vehicle_type: String,
    pub seat: i32,
    pub weight_goods: i32,
    pub weight_all: i32,
    /// Vehicle type (i32, sent to Kafka as JSON number).
    pub vehicle_type: i32,
}

/// Payload to send checkout event in background (owned data when spawning).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutCommitPayload {
    pub request_id: i64,
    pub tid: String,
    pub ticket_in_id: i64,
    pub ticket_eTag_id: i64,
    pub hub_id: Option<i64>,
    pub ticket_out_id: i64,
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
    /// Vehicle type (i32, sent to Kafka as JSON number).
    pub vehicle_type: i32,
    pub ticket_type: String,
    pub price_ticket_type: i32,
    /// Status (i32, sent to Kafka as number).
    pub status: i32,
    pub trans_amount: i32,
    pub rating_detail: Vec<BOORatingDetail>,
}

/// Message type in Kafka queue (one worker processes sequentially).
#[derive(Clone)]
pub enum KafkaMessage {
    Checkin(CheckinCommitPayload),
    Checkout(CheckoutCommitPayload),
}
