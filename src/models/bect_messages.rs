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

// ============== QUERY_VEHICLE_BOO (1A, 0x64, 122 bytes) ==============

/// QUERY_VEHICLE_BOO (1A) – Highway Back-End sends to Card-issuer Back-End to query vehicle and account status at station entry.
/// Command ID: 100 (0x64). Size: 122 bytes. Spec: 2.3.1.7.13.
#[allow(dead_code)]
#[derive(Default)]
pub struct QUERY_VEHICLE_BOO {
    pub message_length: i32,   // 4 – 122
    pub command_id: i32,       // 4 – 100
    pub version_id: i32,       // 4
    pub request_id: i64,       // 8
    pub session_id: i64,       // 8
    pub timestamp: i64,        // 8 – epoch ms when sending
    pub tid: String,           // 24 – TID
    pub etag: String,          // 24 – EPC/ETAG
    pub station: i32,          // 4
    pub lane: i32,             // 4
    pub station_type: String,  // 1 – C: closed, O: open
    pub lane_type: String,     // 1 – I: in, O: out; null for open station
    pub min_balance: i32,      // 4 – required minimum balance, 0 = no requirement
    pub general1: [u8; 8],     // 8
    pub general2: [u8; 16],   // 16
}

impl fmt::Debug for QUERY_VEHICLE_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QUERY_VEHICLE_BOO")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
            .field("tid", &self.tid.trim_end_matches('\0'))
            .field("etag", &self.etag.trim_end_matches('\0'))
            .field("station", &self.station)
            .field("lane", &self.lane)
            .field("station_type", &self.station_type.trim_end_matches('\0'))
            .field("lane_type", &self.lane_type.trim_end_matches('\0'))
            .field("min_balance", &self.min_balance)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

// ============== QUERY_VEHICLE_BOO_RESP (1B, 0x65, 133 bytes) ==============

/// QUERY_VEHICLE_BOO_RESP (1B) – Card-issuer Back-End response to QUERY_VEHICLE_BOO (1A).
/// Command ID: 101 (0x65). Size: 133 bytes. Spec: 2.3.1.7.14.
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

// ============== LOOKUP_VEHICLE (0x96, 110 bytes) ==============

/// LOOKUP_VEHICLE – FE sends to Back-End to lookup vehicle by etag. Command ID: 150 (0x96). Size: 110 bytes.
#[derive(Default)]
pub struct LOOKUP_VEHICLE {
    pub message_length: i32,   // 4 – 110
    pub command_id: i32,       // 4 – 0x96
    pub version_id: i32,       // 4
    pub request_id: i64,       // 8
    pub session_id: i64,       // 8
    pub timestamp: i64,        // 8 – epoch ms
    pub tid: String,           // 24 – TID
    pub etag: String,          // 24 – EPC/ETAG
    pub station: i32,          // 4
    pub lane: i32,             // 4
    pub station_type: String,  // 1 – C: closed, O: open
    pub lane_type: String,     // 1 – I: in, O: out
    pub general1: [u8; 8],     // 8
    pub general2: [u8; 8],     // 8
}

impl fmt::Debug for LOOKUP_VEHICLE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LOOKUP_VEHICLE")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
            .field("tid", &self.tid.trim_end_matches('\0'))
            .field("etag", &self.etag.trim_end_matches('\0'))
            .field("station", &self.station)
            .field("lane", &self.lane)
            .field("station_type", &self.station_type.trim_end_matches('\0'))
            .field("lane_type", &self.lane_type.trim_end_matches('\0'))
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

// ============== LOOKUP_VEHICLE_RESP (0x97, 197 bytes) ==============

/// LOOKUP_VEHICLE_RESP – Back-End response to LOOKUP_VEHICLE. Command ID: 151 (0x97). Size: 197 bytes.
pub struct LOOKUP_VEHICLE_RESP {
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
    /// Extra bytes to reach 197 bytes total (133 + 64).
    pub extra: [u8; 64],
}

impl Default for LOOKUP_VEHICLE_RESP {
    fn default() -> Self {
        Self {
            message_length: 0,
            command_id: 0,
            version_id: 0,
            request_id: 0,
            session_id: 0,
            timestamp: 0,
            process_time: 0,
            etag: String::new(),
            vehicle_type: 0,
            ticket_type: String::new(),
            register_vehicle_type: String::new(),
            seat: 0,
            weight_goods: 0,
            weight_all: 0,
            plate: String::new(),
            status: 0,
            min_balance_status: 0,
            general1: [0u8; 8],
            general2: [0u8; 16],
            extra: [0u8; 64],
        }
    }
}

impl fmt::Debug for LOOKUP_VEHICLE_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LOOKUP_VEHICLE_RESP")
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

// ============== 3A CHECKIN_COMMIT_BOO (0x68, 152 bytes) ==============

/// CHECKIN_COMMIT_BOO (3A) – Highway Back-End sends to Card-issuer Back-End to commit check-in.
/// Command ID: 104 (0x68). Size: 152 bytes. Spec: 2.3.1.7.7.
#[allow(dead_code)]
#[derive(Default)]
pub struct CHECKIN_COMMIT_BOO {
    pub message_length: i32,   // 4 – 152
    pub command_id: i32,       // 4 – 104
    pub version_id: i32,       // 4
    pub request_id: i64,       // 8
    pub session_id: i64,       // 8
    pub timestamp: i64,        // 8 – epoch ms when sending
    pub ticket_id: i64,        // 8 – BOOA transaction ID
    pub ref_trans_id: i64,     // 8 – BOOB unique transaction ID
    pub tid: String,           // 24 – TID
    pub etag: String,          // 24 – EPC/ETAG
    pub station: i32,          // 4
    pub station_type: String,  // 1 – C: closed, O: open
    pub lane_type: String,     // 1 – I: in, O: out; null for open station
    pub lane: i32,             // 4
    pub plate_from_toll: String, // 10 – recognized plate
    pub commit_datetime: i64,   // 8 – epoch ms
    pub general1: [u8; 8],     // 8
    pub general2: [u8; 16],   // 16
}

impl fmt::Debug for CHECKIN_COMMIT_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CHECKIN_COMMIT_BOO")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
            .field("ticket_id", &self.ticket_id)
            .field("ref_trans_id", &self.ref_trans_id)
            .field("tid", &self.tid.trim_end_matches('\0'))
            .field("etag", &self.etag.trim_end_matches('\0'))
            .field("station", &self.station)
            .field("station_type", &self.station_type.trim_end_matches('\0'))
            .field("lane_type", &self.lane_type.trim_end_matches('\0'))
            .field("lane", &self.lane)
            .field("plate_from_toll", &self.plate_from_toll.trim_end_matches('\0'))
            .field("commit_datetime", &self.commit_datetime)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

// ============== COMMIT_BOO_RESP / CHECKIN_COMMIT_BOO_RESP (3B, 0x69, 84 bytes) ==============

/// CHECKIN_COMMIT_BOO_RESP (3B) – Card-issuer Back-End response to CHECKIN_COMMIT_BOO (3A).
/// Highway Back-End receives this from Card-issuer Back-End after commit check-in.
/// Command ID: 105 (0x69). Size: 84 bytes. Spec: 2.3.1.7.8.
#[allow(dead_code)]
pub type CHECKIN_COMMIT_BOO_RESP = COMMIT_BOO_RESP;

/// COMMIT_BOO_RESP (3B) – Same as CHECKIN_COMMIT_BOO_RESP. Command ID: 105 (0x69). Size: 84 bytes.
#[allow(dead_code)]
#[derive(Default)]
pub struct COMMIT_BOO_RESP {
    pub message_length: i32,  // 4 – 84
    pub command_id: i32,      // 4 – 105
    pub version_id: i32,      // 4
    pub request_id: i64,      // 8
    pub session_id: i64,      // 8
    pub timestamp: i64,       // 8 – epoch ms when responding
    pub process_time: i32,    // 4 – millisecond
    pub ticket_id: i64,       // 8 – BOOA transaction ID
    pub ref_trans_id: i64,    // 8 – BOOB unique transaction ID
    pub status: i32,          // 4 – 0: success
    pub general1: [u8; 8],    // 8
    pub general2: [u8; 16],  // 16
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

// ============== 3A CHECKIN_ROLLBACK_BOO (0x6A, 152 bytes) ==============

/// CHECKIN_ROLLBACK_BOO (3A) – Highway Back-End sends to Card-issuer Back-End to rollback check-in.
/// Command ID: 106 (0x6A). Size: 152 bytes. Spec: 2.3.1.7.9.
#[allow(dead_code)]
#[derive(Default)]
pub struct CHECKIN_ROLLBACK_BOO {
    pub message_length: i32,   // 4 – 152
    pub command_id: i32,       // 4 – 106
    pub version_id: i32,       // 4
    pub request_id: i64,       // 8
    pub session_id: i64,       // 8
    pub timestamp: i64,        // 8 – epoch ms when sending
    pub ticket_id: i64,        // 8 – BOOA transaction ID
    pub ref_trans_id: i64,     // 8 – BOOB unique transaction ID
    pub tid: String,           // 24 – TID
    pub etag: String,          // 24 – EPC/ETAG
    pub station: i32,          // 4
    pub station_type: String,  // 1 – C: closed, O: open
    pub lane_type: String,     // 1 – I: in, O: out; null for open station
    pub lane: i32,             // 4
    pub plate_from_toll: String, // 10 – recognized plate
    pub commit_datetime: i64,   // 8 – epoch ms
    pub general1: [u8; 8],     // 8
    pub general2: [u8; 16],   // 16
}

impl fmt::Debug for CHECKIN_ROLLBACK_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CHECKIN_ROLLBACK_BOO")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
            .field("ticket_id", &self.ticket_id)
            .field("ref_trans_id", &self.ref_trans_id)
            .field("tid", &self.tid.trim_end_matches('\0'))
            .field("etag", &self.etag.trim_end_matches('\0'))
            .field("station", &self.station)
            .field("station_type", &self.station_type.trim_end_matches('\0'))
            .field("lane_type", &self.lane_type.trim_end_matches('\0'))
            .field("lane", &self.lane)
            .field("plate_from_toll", &self.plate_from_toll.trim_end_matches('\0'))
            .field("commit_datetime", &self.commit_datetime)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

// ============== ROLLBACK_BOO_RESP / CHECKIN_ROLLBACK_BOO_RESP (3B, 0x6B, 84 bytes) ==============

/// CHECKIN_ROLLBACK_BOO_RESP (3B) – Card-issuer Back-End response to CHECKIN_ROLLBACK_BOO (3A).
/// Highway Back-End receives this from Card-issuer Back-End after rollback check-in.
/// Command ID: 107 (0x6B). Size: 84 bytes. Spec: 2.3.1.7.10.
#[allow(dead_code)]
pub type CHECKIN_ROLLBACK_BOO_RESP = ROLLBACK_BOO_RESP;

/// ROLLBACK_BOO_RESP (3B) – Same as CHECKIN_ROLLBACK_BOO_RESP. Command ID: 107 (0x6B). Size: 84 bytes.
#[allow(dead_code)]
#[derive(Default)]
pub struct ROLLBACK_BOO_RESP {
    pub message_length: i32,  // 4 – 84
    pub command_id: i32,      // 4 – 107
    pub version_id: i32,      // 4
    pub request_id: i64,      // 8
    pub session_id: i64,      // 8
    pub timestamp: i64,       // 8 – epoch ms when responding
    pub process_time: i32,    // 4 – millisecond
    pub ticket_id: i64,       // 8 – BOOA transaction ID
    pub ref_trans_id: i64,    // 8 – BOOB unique transaction ID
    pub status: i32,          // 4 – 0: success
    pub general1: [u8; 8],    // 8
    pub general2: [u8; 16],  // 16
}

impl fmt::Debug for ROLLBACK_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ROLLBACK_BOO_RESP")
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

// ============== CHECKOUT_RESERVE_BOO (2AZ, variable) ==============

/// CHECKOUT_RESERVE_BOO (2AZ) – Exit-station Back-End sends to card-issuer Back-End to request reserve amount for checkout.
/// Command ID: 152 (0x98). Size: variable (depends on rating_detail_line). Spec: 2.3.1.7.17.
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

// ============== CHECKOUT_RESERVE_BOO_RESP (2BZ, 100 bytes) ==============

/// CHECKOUT_RESERVE_BOO_RESP (2BZ) – Card-issuer Back-End response to CHECKOUT_RESERVE_BOO (2AZ).
/// Command ID: 153 (0x99). Size: 100 bytes. Spec: 2.3.1.7.18. Status 0 = success.
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

// ============== CHECKOUT_COMMIT_BOO (3AZ, 0x9A, 188 bytes) ==============

/// CHECKOUT_COMMIT_BOO (3AZ) – Exit-station Back-End sends to card-issuer Back-End to request commit of checkout.
/// Command ID: 154 (0x9A). Size: 188 bytes. Spec: 2.3.1.7.19.
#[allow(dead_code)]
#[derive(Default)]
pub struct CHECKOUT_COMMIT_BOO {
    pub message_length: i32,   // 4 – 188
    pub command_id: i32,       // 4 – 154
    pub version_id: i32,        // 4
    pub request_id: i64,        // 8
    pub session_id: i64,        // 8
    pub timestamp: i64,         // 8 – epoch ms when sending
    pub tid: String,            // 24 – TID
    pub etag: String,           // 24 – EPC/ETAG
    pub ticket_in_id: i64,      // 8 – entry station transaction ID
    pub hub_id: i64,            // 8 – hub sync ID
    pub ticket_out_id: i64,     // 8 – exit station transaction ID
    pub ticket_eTag_id: i64,    // 8 – BOO card transaction ID
    pub station_in: i32,        // 4 – entry station
    pub lane_in: i32,           // 4 – entry lane
    pub station_out: i32,       // 4 – exit station
    pub lane_out: i32,          // 4 – exit lane
    pub plate: String,          // 20 – plate number
    pub trans_amount: i32,      // 4 – amount to deduct
    /// Epoch datetime in seconds (per spec).
    pub trans_datetime: i64,    // 8
    pub general1: [u8; 8],      // 8
    pub general2: [u8; 16],     // 16
}

impl fmt::Debug for CHECKOUT_COMMIT_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CHECKOUT_COMMIT_BOO")
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
            .field("ticket_out_id", &self.ticket_out_id)
            .field("ticket_eTag_id", &self.ticket_eTag_id)
            .field("station_in", &self.station_in)
            .field("lane_in", &self.lane_in)
            .field("station_out", &self.station_out)
            .field("lane_out", &self.lane_out)
            .field("plate", &self.plate.trim_end_matches('\0'))
            .field("trans_amount", &self.trans_amount)
            .field("trans_datetime", &self.trans_datetime)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

// ============== CHECKOUT_COMMIT_BOO_RESP (3BZ, 0x9B, 96 bytes) ==============

/// CHECKOUT_COMMIT_BOO_RESP (3BZ) – Card-issuer Back-End response to CHECKOUT_COMMIT_BOO (3AZ).
/// Command ID: 155 (0x9B). Size: 96 bytes. Spec: 2.3.1.7.20. Status 0 = success.
#[allow(dead_code)]
#[derive(Default)]
pub struct CHECKOUT_COMMIT_BOO_RESP {
    pub message_length: i32,  // 4 – 96
    pub command_id: i32,      // 4 – 155
    pub version_id: i32,       // 4
    pub request_id: i64,       // 8
    pub session_id: i64,       // 8
    pub timestamp: i64,        // 8 – epoch ms when responding
    pub ticket_in_id: i64,     // 8
    pub hub_id: i64,          // 8
    pub ticket_eTag_id: i64,   // 8
    pub ticket_out_id: i64,    // 8
    pub status: i32,           // 4 – 0: success
    pub general1: [u8; 8],     // 8
    pub general2: [u8; 16],    // 16
}

impl fmt::Debug for CHECKOUT_COMMIT_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CHECKOUT_COMMIT_BOO_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
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

// ============== CHECKOUT_ROLLBACK_BOO (3AZ, 0x9C, 178 bytes) ==============

/// CHECKOUT_ROLLBACK_BOO (3AZ) – Exit-station Back-End sends to card-issuer Back-End to request rollback of checkout.
/// Command ID: 156 (0x9C). Size: 178 bytes. Spec: 2.3.1.7.21.
#[allow(dead_code)]
#[derive(Default)]
pub struct CHECKOUT_ROLLBACK_BOO {
    pub message_length: i32,   // 4 – 178
    pub command_id: i32,       // 4 – 156
    pub version_id: i32,        // 4
    pub request_id: i64,        // 8
    pub session_id: i64,       // 8
    pub timestamp: i64,        // 8 – epoch ms when sending
    pub tid: String,           // 24 – TID
    pub etag: String,          // 24 – EPC/ETAG
    pub ticket_in_id: i64,     // 8 – entry station transaction ID
    pub hub_id: i64,           // 8 – hub sync ID
    pub ticket_out_id: i64,    // 8 – exit station transaction ID
    pub ticket_eTag_id: i64,   // 8 – BOO card transaction ID
    pub station_in: i32,       // 4 – entry station
    pub lane_in: i32,          // 4 – entry lane
    pub station_out: i32,      // 4 – exit station
    pub lane_out: i32,         // 4 – exit lane
    pub plate: String,         // 10 – plate number
    pub trans_amount: i32,     // 4 – amount to deduct
    pub trans_datetime: i64,   // 8 – epoch datetime in seconds
    pub general1: [u8; 8],     // 8
    pub general2: [u8; 16],   // 16
}

impl fmt::Debug for CHECKOUT_ROLLBACK_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CHECKOUT_ROLLBACK_BOO")
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
            .field("ticket_out_id", &self.ticket_out_id)
            .field("ticket_eTag_id", &self.ticket_eTag_id)
            .field("station_in", &self.station_in)
            .field("lane_in", &self.lane_in)
            .field("station_out", &self.station_out)
            .field("lane_out", &self.lane_out)
            .field("plate", &self.plate.trim_end_matches('\0'))
            .field("trans_amount", &self.trans_amount)
            .field("trans_datetime", &self.trans_datetime)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

// ============== CHECKOUT_ROLLBACK_BOO_RESP (3BZ, 0x9D, 96 bytes) ==============

/// CHECKOUT_ROLLBACK_BOO_RESP (3BZ) – Card-issuer Back-End response to CHECKOUT_ROLLBACK_BOO (3AZ).
/// Command ID: 157 (0x9D). Size: 96 bytes. Spec: 2.3.1.7.22. Status 0 = success.
#[allow(dead_code)]
#[derive(Default)]
pub struct CHECKOUT_ROLLBACK_BOO_RESP {
    pub message_length: i32,  // 4 – 96
    pub command_id: i32,      // 4 – 157
    pub version_id: i32,       // 4
    pub request_id: i64,       // 8
    pub session_id: i64,       // 8
    pub timestamp: i64,        // 8 – epoch ms when responding
    pub ticket_in_id: i64,     // 8
    pub hub_id: i64,          // 8
    pub ticket_eTag_id: i64,   // 8
    pub ticket_out_id: i64,    // 8
    pub status: i32,           // 4 – 0: success
    pub general1: [u8; 8],     // 8
    pub general2: [u8; 16],   // 16
}

impl fmt::Debug for CHECKOUT_ROLLBACK_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CHECKOUT_ROLLBACK_BOO_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
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
