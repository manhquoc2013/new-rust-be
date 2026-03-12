//! BECT protocol message structures per spec (2A/2B and related).
//! Unified types used for BECT/BOO flow; replaces former VDTCmessages/VETCmessages usage.

#![allow(non_camel_case_types)]
#![allow(dead_code)]

use std::fmt;

use crate::models::rating_detail::RatingDetail;

// ============== 2A CHECKIN_RESERVE_BOO (0x66, 172 bytes) ==============

/// CHECKIN_RESERVE_BOO (2A) – Back-End sends to card-issuer to reserve balance at check-in.
/// Command ID: 102 (0x66). Size: 172 bytes.
#[allow(dead_code)]
#[derive(Default)]
pub struct CHECKIN_RESERVE_BOO {
    pub message_length: i32,   // 4 – 172
    pub command_id: i32,      // 4 – 102
    pub version_id: i32,      // 4
    pub request_id: i64,      // 8
    pub session_id: i64,      // 8
    pub timestamp: i64,       // 8 – epoch ms
    pub ticket_id_in: i64,    // 8 – BOOA unique transaction ID
    pub tid: String,          // 24 – TID
    pub etag: String,         // 24 – EPC/ETAG (spec: eTag)
    pub station: i32,         // 4
    pub lane: i32,            // 4
    pub station_type: String, // 1 – C: closed, O: open
    pub lane_type: String,    // 1 – I: in, O: out; null for open
    pub vehicle_type: i32,    // 4
    pub ticket_type: String,  // 1 – L, T, Q, N
    pub price_ticket_type: i32, // 4 – discount ticket ID
    pub subscription_id: String, // 25 – subscription ID(s), comma-separated
    pub trans_amount: i32,    // 4
    /// Epoch datetime in **seconds** (per spec).
    pub trans_datetime: i64,  // 8
    pub general1: [u8; 8],   // 8
    pub general2: [u8; 16],  // 16
}

impl fmt::Debug for CHECKIN_RESERVE_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CHECKIN_RESERVE_BOO")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
            .field("ticket_id_in", &self.ticket_id_in)
            .field("tid", &self.tid.trim_end_matches('\0'))
            .field("etag", &self.etag.trim_end_matches('\0'))
            .field("station", &self.station)
            .field("lane", &self.lane)
            .field("station_type", &self.station_type.trim_end_matches('\0'))
            .field("lane_type", &self.lane_type.trim_end_matches('\0'))
            .field("vehicle_type", &self.vehicle_type)
            .field("ticket_type", &self.ticket_type.trim_end_matches('\0'))
            .field("price_ticket_type", &self.price_ticket_type)
            .field("subscription_id", &self.subscription_id.trim_end_matches('\0'))
            .field("trans_amount", &self.trans_amount)
            .field("trans_datetime", &self.trans_datetime)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

// ============== 2B CHECKIN_RESERVE_BOO_RESP (0x67, 76 bytes) ==============

/// CHECKIN_RESERVE_BOO_RESP (2B) – Card-issuer response to check-in reserve.
/// Command ID: 103 (0x67). Size: 76 bytes.
#[allow(dead_code)]
#[derive(Default)]
pub struct CHECKIN_RESERVE_BOO_RESP {
    pub message_length: i32, // 4 – 76
    pub command_id: i32,     // 4 – 103
    pub version_id: i32,     // 4
    pub request_id: i64,     // 8
    pub session_id: i64,     // 8
    pub timestamp: i64,      // 8 – epoch ms
    pub process_time: i32,  // 4 – ms
    pub ref_trans_id: i64,    // 8 – BOOB unique transaction ID
    pub status: i32,         // 4 – 0: success, other: failure
    pub general1: [u8; 8],   // 8
    pub general2: [u8; 16],  // 16
}

impl fmt::Debug for CHECKIN_RESERVE_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CHECKIN_RESERVE_BOO_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
            .field("process_time", &self.process_time)
            .field("ref_trans_id", &self.ref_trans_id)
            .field("status", &self.status)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

// ============== QUERY_VEHICLE_BOO_RESP (0x65, 133 bytes) ==============

#[allow(dead_code)]
#[derive(Default)]
pub struct QUERY_VEHICLE_BOO_RESP {
    pub message_length: i32,
    pub command_id: i32,
    pub version_id: i32,
    pub request_id: i64,
    pub session_id: i64,
    pub timestamp: i64,
    pub process_time: i32,
    pub etag: String,
    pub vehicle_type: i32,
    pub ticket_type: String,
    pub register_vehicle_type: String,
    pub seat: i32,
    pub weight_goods: i32,
    pub weight_all: i32,
    pub plate: String,
    pub status: i32,
    pub min_balance_status: i32,
    pub general1: [u8; 8],
    pub general2: [u8; 16],
}

impl fmt::Debug for QUERY_VEHICLE_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QUERY_VEHICLE_BOO_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
            .field("process_time", &self.process_time)
            .field("etag", &self.etag.trim_end_matches('\0'))
            .field("vehicle_type", &self.vehicle_type)
            .field("ticket_type", &self.ticket_type.trim_end_matches('\0'))
            .field("register_vehicle_type", &self.register_vehicle_type.trim_end_matches('\0'))
            .field("seat", &self.seat)
            .field("weight_goods", &self.weight_goods)
            .field("weight_all", &self.weight_all)
            .field("plate", &self.plate.trim_end_matches('\0'))
            .field("status", &self.status)
            .field("min_balance_status", &self.min_balance_status)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

// ============== COMMIT_BOO_RESP (0x69, 84 bytes) ==============

#[allow(dead_code)]
#[derive(Default)]
pub struct COMMIT_BOO_RESP {
    pub message_length: i32,
    pub command_id: i32,
    pub version_id: i32,
    pub request_id: i64,
    pub session_id: i64,
    pub timestamp: i64,
    pub process_time: i32,
    pub ticket_id: i64,
    pub ref_trans_id: i64,
    pub status: i32,
    pub general1: [u8; 8],
    pub general2: [u8; 16],
}

impl fmt::Debug for COMMIT_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("COMMIT_BOO_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
            .field("process_time", &self.process_time)
            .field("ticket_id", &self.ticket_id)
            .field("ref_trans_id", &self.ref_trans_id)
            .field("status", &self.status)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

// ============== CHECKOUT_RESERVE_BOO (variable) ==============

#[allow(dead_code)]
#[derive(Default)]
pub struct CHECKOUT_RESERVE_BOO {
    pub message_length: i32,
    pub command_id: i32,
    pub version_id: i32,
    pub request_id: i64,
    pub session_id: i64,
    pub timestamp: i64,
    pub tid: String,
    pub etag: String,
    pub ticket_in_id: i64,
    pub hub_id: Option<i64>,
    pub ticket_eTag_id: i64,
    pub ticket_out_id: i64,
    pub checkin_datetime: i64,
    pub checkin_commit_datetime: i64,
    pub station_in: i32,
    pub lane_in: i32,
    pub station_out: i32,
    pub lane_out: i32,
    pub plate: String,
    pub ticket_type: String,
    pub price_ticket_type: i32,
    pub trans_amount: i32,
    pub trans_datetime: i64,
    pub rating_detail_line: i32,
    pub rating_detail: Vec<RatingDetail>,
    pub general1: [u8; 8],
    pub general2: [u8; 16],
}

impl fmt::Debug for CHECKOUT_RESERVE_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CHECKOUT_RESERVE_BOO")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
            .field("tid", &self.tid.trim_end_matches('\0'))
            .field("etag", &self.etag.trim_end_matches('\0'))
            .field("ticket_in_id", &self.ticket_in_id)
            .field("hub_id", &self.hub_id)
            .field("ticket_eTag_id", &self.ticket_eTag_id)
            .field("ticket_out_id", &self.ticket_out_id)
            .field("checkin_datetime", &self.checkin_datetime)
            .field("checkin_commit_datetime", &self.checkin_commit_datetime)
            .field("station_in", &self.station_in)
            .field("lane_in", &self.lane_in)
            .field("station_out", &self.station_out)
            .field("lane_out", &self.lane_out)
            .field("plate", &self.plate.trim_end_matches('\0'))
            .field("ticket_type", &self.ticket_type.trim_end_matches('\0'))
            .field("price_ticket_type", &self.price_ticket_type)
            .field("trans_amount", &self.trans_amount)
            .field("trans_datetime", &self.trans_datetime)
            .field("rating_detail_line", &self.rating_detail_line)
            .field("rating_detail", &self.rating_detail)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

// ============== CHECKOUT_RESERVE_BOO_RESP (100 bytes) ==============

#[allow(dead_code)]
#[derive(Default)]
pub struct CHECKOUT_RESERVE_BOO_RESP {
    pub message_length: i32,
    pub command_id: i32,
    pub version_id: i32,
    pub request_id: i64,
    pub session_id: i64,
    pub timestamp: i64,
    pub process_time: i32,
    pub ticket_in_id: i64,
    pub hub_id: Option<i64>,
    pub ticket_eTag_id: i64,
    pub ticket_out_id: i64,
    pub status: i32,
    pub general1: [u8; 8],
    pub general2: [u8; 16],
}

impl fmt::Debug for CHECKOUT_RESERVE_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CHECKOUT_RESERVE_BOO_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
            .field("process_time", &self.process_time)
            .field("ticket_in_id", &self.ticket_in_id)
            .field("hub_id", &self.hub_id)
            .field("ticket_eTag_id", &self.ticket_eTag_id)
            .field("ticket_out_id", &self.ticket_out_id)
            .field("status", &self.status)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}
