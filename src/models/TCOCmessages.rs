//! Protocol message structures FE (CONNECT, HANDSHAKE, CHECKIN, COMMIT, ROLLBACK, TERMINATE, ...).
#![allow(non_camel_case_types)]

use std::fmt;

#[derive(Default, Clone)]
pub struct FE_REQUEST {
    pub message_length: i32,
    pub command_id: i32,
    pub request_id: i64,
    pub session_id: i64,
}
impl fmt::Debug for FE_REQUEST {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_REQUEST")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .finish()
    }
}
#[derive(Default)]
pub struct FE_CONNECT {
    pub message_length: i32,
    pub command_id: i32,
    /// Protocol version, 4 bytes. Total message 44 bytes (2.3.1.7.1).
    pub version_id: i32,
    pub request_id: i64,
    pub username: String,
    pub password: String,
    pub timeout: i32,
}
impl fmt::Debug for FE_CONNECT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_CONNECT")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("username", &self.username.trim_end_matches('\0'))
            .field("password", &self.password.trim_end_matches('\0'))
            .field("timeout", &self.timeout)
            .finish()
    }
}
#[derive(Default)]
pub struct FE_CONNECT_RESP {
    pub message_length: i32,
    pub command_id: i32,
    /// Protocol version, 4 bytes. Total message 32 bytes (2.3.1.7.2).
    pub version_id: i32,
    pub request_id: i64,
    pub session_id: i64,
    /// 0: success; non-zero: auth/error code (e.g. 301, 305, 306).
    pub status: i32,
}
impl fmt::Debug for FE_CONNECT_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_CONNECT_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Default)]
pub struct FE_SHAKE {
    pub message_length: i32,
    pub command_id: i32,
    /// Protocol version, 4 bytes. Total message 28 bytes (2.3.1.7.3).
    pub version_id: i32,
    pub request_id: i64,
    pub session_id: i64,
}
impl fmt::Debug for FE_SHAKE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_SHAKE")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .finish()
    }
}
#[derive(Default)]
pub struct FE_SHAKE_RESP {
    pub message_length: i32,
    pub command_id: i32,
    /// Protocol version, 4 bytes. Total message 32 bytes (2.3.1.7.4).
    pub version_id: i32,
    pub request_id: i64,
    pub session_id: i64,
    /// 0: success; 1: session expired, close connection.
    pub status: i32,
}

impl fmt::Debug for FE_SHAKE_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_SHAKE_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("status", &self.status)
            .finish()
    }
}
#[derive(Default)]
pub struct FE_CHECKIN {
    pub message_length: i32,
    pub command_id: i32,
    pub request_id: i64,
    pub session_id: i64,

    /// eTag EPC, 24 bytes.
    pub etag: String,
    /// Toll station code, 4 bytes.
    pub station: i32,
    /// Lane code (optional), 4 bytes.
    pub lane: i32,
    /// Plate number from FE, 10 bytes.
    pub plate: String,
    /// Transaction id from reader (optional), 24 bytes.
    pub tid: String,
    /// eTag security value (optional), 16 bytes.
    pub hash_value: String,
}

impl fmt::Debug for FE_CHECKIN {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_CHECKIN")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("etag", &self.etag.trim_end_matches('\0'))
            .field("station", &self.station)
            .field("lane", &self.lane)
            .field("plate", &self.plate.trim_end_matches('\0'))
            .field("tid", &self.tid.trim_end_matches('\0'))
            .field("hash_value", &self.hash_value.trim_end_matches('\0'))
            .finish()
    }
}
#[derive(Default)]
pub struct FE_CHECKIN_IN_RESP {
    pub message_length: i32,
    pub command_id: i32,
    pub request_id: i64,
    pub session_id: i64,

    /// Status: 0 = success.
    pub status: i32,
    /// Vehicle eTag, 24 bytes.
    pub etag: String,
    /// Station code, 4 bytes.
    pub station: i32,
    /// Lane code, 4 bytes.
    pub lane: i32,
    /// Ticket id from Back End, 8 bytes (Long).
    pub ticket_id: i64,
    /// Ticket type (month, trip, quarter, year), 4 bytes.
    pub ticket_type: i32,
    /// Toll amount to deduct, 4 bytes.
    pub price: i32,
    /// Vehicle type, 4 bytes.
    pub vehicle_type: i32,
    /// Plate number, 10 bytes.
    pub plate: String,
    /// Plate type (white, blue, red, etc.), 4 bytes.
    pub plate_type: i32,
    /// Discount price type id, 4 bytes.
    pub price_ticket_type: i32,
}

impl fmt::Debug for FE_CHECKIN_IN_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_CHECKIN_IN_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("status", &self.status)
            .field("etag", &self.etag.trim_end_matches('\0'))
            .field("station", &self.station)
            .field("lane", &self.lane)
            .field("ticket_id", &self.ticket_id)
            .field("ticket_type", &self.ticket_type)
            .field("price", &self.price)
            .field("vehicle_type", &self.vehicle_type)
            .field("plate", &self.plate.trim_end_matches('\0'))
            .field("plate_type", &self.plate_type)
            .field("price_ticket_type", &self.price_ticket_type)
            .finish()
    }
}

/// FE COMMIT – Front End request to deduct vehicle account at station.
///
/// **REQUEST HEADER**
/// - Message Length = 98 (fixed)
/// - Command Id: 0x06
/// - Request Id: unique from Front End (Long, 8 bytes)
/// - Session Id: from CONNECT RESP (Long, 8 bytes)
///
/// **REQUEST BODY** (byte layout per spec)
///
/// | Field             | Bytes | Type   | Mandatory | Description |
/// |-------------------|-------|--------|-----------|-------------|
/// | Message Length    | 4     | Int    | M         | Fixed 98 bytes |
/// | Command Id        | 4     | Int    | M         | 0x06 |
/// | Request Id        | 8     | Long   | M         | Unique id from FE |
/// | Session Id        | 8     | Long   | M         | Session from BE |
/// | Etag              | 24    | String | M         | Vehicle eTag EPC |
/// | Station           | 4     | Int    | M         | Toll station in use |
/// | Lane              | 4     | Int    | O         | Lane code; 0 if unknown |
/// | Ticket Id         | 8     | Long   | M         | From CHECKIN RESP (Reserve) |
/// | Status            | 4     | Int    | M         | Plate match status (image comparison) |
/// | Plate             | 10    | String | O         | Actual plate from camera |
/// | Image Count       | 4     | Int    | O         | Number of images; names TicketId_0x |
/// | Vehicle Length    | 4     | Int    | O         | Length (cm); 0 if unknown |
/// | Transaction Amount| 4     | Int    | O         | Amount FE calculated; 0 = use BE from CHECKIN_RESP |
/// | Weight            | 4     | Int    | O         | Weight; 0 if unknown |
/// | Reason ID         | 4     | Int    | O         | Commit/rollback reason; default 0 |
///
/// **Total: 98 bytes**
#[derive(Default)]
pub struct FE_COMMIT_IN {
    pub message_length: i32,
    pub command_id: i32,
    pub request_id: i64,
    pub session_id: i64,

    /// eTag EPC, 24 bytes. (M)
    pub etag: String,
    /// Toll station in use, 4 bytes. (M)
    pub station: i32,
    /// Lane when vehicle passed (optional), 4 bytes. (O) 0 if unknown.
    pub lane: i32,
    /// Ticket id from CHECKIN RESP, 8 bytes (Long). (M)
    pub ticket_id: i64,
    /// Plate match status (image comparison), 4 bytes. (M)
    pub status: i32,
    /// Actual plate from camera, 10 bytes. (O) May be empty.
    pub plate: String,
    /// Image count; names TicketId_0x, 4 bytes. (O)
    pub image_count: i32,
    /// Vehicle length (cm), 4 bytes. (O) 0 if unknown.
    pub vehicle_length: i32,
    /// Amount FE calculated; 0 = BE deducts per CHECKIN_RESP, 4 bytes. (O)
    pub transaction_amount: i32,
    /// Weight, 4 bytes. (O) 0 if unknown.
    pub weight: i32,
    /// Commit/rollback reason, special stations; default 0, 4 bytes. (O)
    pub reason_id: i32,
}

impl fmt::Debug for FE_COMMIT_IN {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_COMMIT_IN")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("etag", &self.etag.trim_end_matches('\0'))
            .field("station", &self.station)
            .field("lane", &self.lane)
            .field("ticket_id", &self.ticket_id)
            .field("status", &self.status)
            .field("plate", &self.plate.trim_end_matches('\0'))
            .field("image_count", &self.image_count)
            .field("vehicle_length", &self.vehicle_length)
            .field("transaction_amount", &self.transaction_amount)
            .field("weight", &self.weight)
            .field("reason_id", &self.reason_id)
            .finish()
    }
}
#[derive(Default)]
pub struct FE_COMMIT_IN_RESP {
    pub message_length: i32,
    pub command_id: i32,
    pub request_id: i64,
    pub session_id: i64,
    /// Plate match status (0: no match, 1: match, 2: unrecognized), 4 bytes.
    pub status: i32,
}

impl fmt::Debug for FE_COMMIT_IN_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_COMMIT_IN_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Default)]
pub struct FE_ROLLBACK {
    pub message_length: i32,
    pub command_id: i32,
    pub request_id: i64,
    pub session_id: i64,

    /// eTag EPC, 24 bytes.
    pub etag: String,
    /// Toll station code, 4 bytes.
    pub station: i32,
    /// Lane code (optional), 4 bytes. 0 if lane unknown at entry (e.g. remote gantry).
    pub lane: i32,
    /// Ticket id from Back End (CHECKIN), 8 bytes (Long).
    pub ticket_id: i64,
    /// Plate match status, 4 bytes.
    pub status: i32,
    /// Actual plate (optional), 10 bytes. May be null if FE could not recognize.
    pub plate: String,
    /// Image count (optional), 4 bytes. Names: TicketId_0x (X = Image Count).
    pub image_count: i32,
    /// Weight (optional), 4 bytes. 0 if unknown.
    pub weight: i32,
    /// Commit/rollback reason for special stations (optional), 4 bytes. Default 0.
    pub reason_id: i32,
}

impl fmt::Debug for FE_ROLLBACK {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_ROLLBACK")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("etag", &self.etag.trim_end_matches('\0'))
            .field("station", &self.station)
            .field("lane", &self.lane)
            .field("ticket_id", &self.ticket_id)
            .field("status", &self.status)
            .field("plate", &self.plate.trim_end_matches('\0'))
            .field("image_count", &self.image_count)
            .field("weight", &self.weight)
            .field("reason_id", &self.reason_id)
            .finish()
    }
}

#[derive(Default)]
pub struct FE_ROLLBACK_RESP {
    pub message_length: i32,
    pub command_id: i32,
    pub request_id: i64,
    pub session_id: i64,
    /// Rollback status (0: success, non-zero: failure), 4 bytes.
    pub status: i32,
}

impl fmt::Debug for FE_ROLLBACK_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_ROLLBACK_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Default)]
pub struct FE_TERMINATE {
    pub message_length: i32,
    pub command_id: i32,
    pub request_id: i64,
    pub session_id: i64,
}

impl fmt::Debug for FE_TERMINATE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_TERMINATE")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .finish()
    }
}

#[derive(Default)]
pub struct FE_TERMINATE_RESP {
    pub message_length: i32,
    pub command_id: i32,
    pub request_id: i64,
    pub session_id: i64,
    /// Terminate status (0: success, non-zero: failure), 4 bytes.
    pub status: i32,
}

impl fmt::Debug for FE_TERMINATE_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_TERMINATE_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("status", &self.status)
            .finish()
    }
}
