//! Protocol message structures VETC (BOO1): CONNECT, HANDSHAKE, CHECKIN_RESERVE_BOO, COMMIT_BOO, ...
#![allow(non_camel_case_types)]
#![allow(dead_code)]

use std::fmt;

#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_REQUEST {
    pub message_length: i32,
    pub command_id: i32,
    pub version_id: i32,
    pub request_id: i64,
    pub session_id: i64,
}

impl fmt::Debug for VETC_REQUEST {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_REQUEST")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .finish()
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_CONNECT {
    pub message_length: i32,
    pub command_id: i32,
    pub version_id: i32,
    pub request_id: i64,
    pub username: String,
    pub password: String,
    pub timeout: i32,
}

impl fmt::Debug for VETC_CONNECT {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_CONNECT")
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

#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_CONNECT_RESP {
    pub message_length: i32,
    pub command_id: i32,
    pub version_id: i32,
    pub request_id: i64,
    pub session_id: i64,
    pub status: i32,
}

impl fmt::Debug for VETC_CONNECT_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_CONNECT_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("status", &self.status)
            .finish()
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_SHAKE {
    pub message_length: i32,
    pub command_id: i32,
    pub version_id: i32,
    pub request_id: i64,
    pub session_id: i64,
}

impl fmt::Debug for VETC_SHAKE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_SHAKE")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .finish()
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_SHAKE_RESP {
    pub message_length: i32,
    pub command_id: i32,
    pub version_id: i32,
    pub request_id: i64,
    pub session_id: i64,
    pub status: i32,
}

impl fmt::Debug for VETC_SHAKE_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_SHAKE_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("status", &self.status)
            .finish()
    }
}

/// VETC_TERMINATE - Bản tin terminate
#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_TERMINATE {
    pub message_length: i32, // 4 bytes - 28
    pub command_id: i32,     // 4 bytes - Mã bản tin: 112
    pub version_id: i32,     // 4 bytes - Phiên bản bản tin
    pub request_id: i64,     // 8 bytes - Long - Id duy nhất do Client sinh ra
    pub session_id: i64,     // 8 bytes - Long - Session ID
}

impl fmt::Debug for VETC_TERMINATE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_TERMINATE")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .finish()
    }
}

/// VETC_TERMINATE_RESP - Bản tin terminate response
#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_TERMINATE_RESP {
    pub message_length: i32, // 4 bytes - 24
    pub command_id: i32,     // 4 bytes - Mã bản tin: 113
    pub version_id: i32,     // 4 bytes - Phiên bản bản tin
    pub request_id: i64, // 8 bytes - Long - Id duy nhất do Client sinh ra, Back-End sẽ trả về bản tin nội dung Request Id do Client gửi đến
    pub session_id: i64, // 8 bytes - Long - Session ID
    pub status: i32, // 4 bytes - Int - Trạng thái đóng kết nối. 0: thành công, 1: không thành công
}

impl fmt::Debug for VETC_TERMINATE_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_TERMINATE_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("status", &self.status)
            .finish()
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_CHECKIN_RESERVE_BOO {
    pub message_length: i32,     // 4 bytes - 151
    pub command_id: i32,         // 4 bytes - Mã bản tin: 102
    pub version_id: i32,         // 4 bytes - Phiên bản bản tin
    pub request_id: i64,         // 8 bytes - Long
    pub session_id: i64,         // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi gửi bản tin (millisecond)
    pub ticket_id_in: i64, // 8 bytes - Long - Mã giao dịch unique của BOOA
    pub tid: String,    // 24 bytes - String - Mã TID
    pub etag: String,   // 24 bytes - String - Mã EPC/ETAG
    pub station: i32,   // 4 bytes - Int - Mã trạm
    pub lane: i32,      // 4 bytes - Int - Mã Làn
    pub station_type: String, // 1 byte - String - Trạm kín: C, Trạm mở: O
    pub lane_type: String, // 1 byte - String - Làn Trạm mở: để null, Trạm kín: Làn vào: I (in), Làn ra: O (out)
    pub vehicle_type: i32, // 4 bytes - Int - Loại xe
    pub ticket_type: String, // 1 byte - String - Loại vé: L, T, Q, N
    pub price_ticket_type: i32, // 4 bytes - Int - ID của vé miễn giảm
    pub subscription_id: String, // 25 bytes - String - ID của vé tháng (có thể nhiều vé cách nhau bởi dấu ",")
    pub trans_amount: i32,       // 4 bytes - Int - Số tiền cần trừ phí
    pub trans_datetime: i64,     // 8 bytes - Long - epoch datetime, millisecond
    pub general1: [u8; 8],       // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16],      // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_CHECKIN_RESERVE_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_CHECKIN_RESERVE_BOO")
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
            .field(
                "subscription_id",
                &self.subscription_id.trim_end_matches('\0'),
            )
            .field("trans_amount", &self.trans_amount)
            .field("trans_datetime", &self.trans_datetime)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_QUERY_VEHICLE_BOO {
    pub message_length: i32,  // 4 bytes - 122
    pub command_id: i32,      // 4 bytes - Mã bản tin: 100
    pub version_id: i32,      // 4 bytes - Phiên bản bản tin
    pub request_id: i64,      // 8 bytes - Long
    pub session_id: i64,      // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi gửi bản tin (millisecond)
    pub tid: String,    // 24 bytes - String - Mã TID
    pub etag: String,   // 24 bytes - String - Mã EPC/ETAG
    pub station: i32,   // 4 bytes - Int - Mã trạm vào
    pub lane: i32,      // 4 bytes - Int - Mã làn vào
    pub station_type: String, // 1 byte - String - Trạm kín: C, Trạm mở: O
    pub lane_type: String, // 1 byte - String - Làn Trạm mở: để null, Trạm kín: Làn vào: I (in), làn ra: O (out)
    pub min_balance: i32,  // 4 bytes - Int - Yêu cầu số dư tối thiểu, không yêu cầu để = 0
    pub general1: [u8; 8], // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16], // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_QUERY_VEHICLE_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_QUERY_VEHICLE_BOO")
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

#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_QUERY_VEHICLE_BOO_RESP {
    pub message_length: i32,           // 4 bytes - 133
    pub command_id: i32,               // 4 bytes - Mã bản tin: 101
    pub version_id: i32,               // 4 bytes - Phiên bản bản tin
    pub request_id: i64,               // 8 bytes - Long
    pub session_id: i64,               // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi phản hồi bản tin (millisecond)
    pub process_time: i32, // 4 bytes - Int - thời gian xử lý tính theo millisecond
    pub etag: String,   // 24 bytes - String - Mã EPC/ETAG
    pub vehicle_type: i32, // 4 bytes - Int - Loại xe tiêu chuẩn
    pub ticket_type: String, // 1 byte - String - Loại vé: L, T, Q, N
    pub register_vehicle_type: String, // 10 bytes - String - Loại xe đăng kiểm
    pub seat: i32,      // 4 bytes - Int - Số ghế
    pub weight_goods: i32, // 4 bytes - Int - Cân nặng hàng hoá
    pub weight_all: i32, // 4 bytes - Int - Cân nặng toàn bộ
    pub plate: String,  // 10 bytes - String - Biển số
    pub status: i32, // 4 bytes - Int - Trạng thái: 0: thành công (cho phép dùng để tính phí xe qua trạm, nếu khác 0 không cho phép qua trạm)
    pub min_balance_status: i32, // 4 bytes - Int - Trạng thái số dư tối thiểu: 0: đủ, 1: không đủ
    pub general1: [u8; 8], // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16], // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_QUERY_VEHICLE_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_QUERY_VEHICLE_BOO_RESP")
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
            .field(
                "register_vehicle_type",
                &self.register_vehicle_type.trim_end_matches('\0'),
            )
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

/// LOOKUP_VEHICLE (1AZ) - 110 byte. Trạm ra gửi BOO để tìm bản ghi check-in khi không tìm thấy.
#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_LOOKUP_VEHICLE {
    pub message_length: i32,  // 4 - 110
    pub command_id: i32,      // 4 - 150
    pub version_id: i32,      // 4
    pub request_id: i64,      // 8
    pub session_id: i64,      // 8
    pub timestamp: i64,       // 8 - epoch ms
    pub tid: String,          // 24
    pub etag: String,         // 24
    pub station_type: String, // 1 - C/O
    pub lane_type: String,    // 1 - I/O
    pub general1: [u8; 8],    // 8
    pub general2: [u8; 16],   // 16
}

impl fmt::Debug for VETC_LOOKUP_VEHICLE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_LOOKUP_VEHICLE")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("version_id", &self.version_id)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("timestamp", &self.timestamp)
            .field("tid", &self.tid.trim_end_matches('\0'))
            .field("etag", &self.etag.trim_end_matches('\0'))
            .field("station_type", &self.station_type.trim_end_matches('\0'))
            .field("lane_type", &self.lane_type.trim_end_matches('\0'))
            .finish()
    }
}

/// LOOKUP_VEHICLE_RESP (1BZ) - 197 byte. BOO trả về dữ liệu check-in.
#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_LOOKUP_VEHICLE_RESP {
    pub message_length: i32,           // 4 - 197
    pub command_id: i32,               // 4 - 151
    pub version_id: i32,               // 4
    pub request_id: i64,               // 8
    pub session_id: i64,               // 8
    pub timestamp: i64,                // 8
    pub tid: String,                   // 24
    pub etag: String,                  // 24
    pub plate: String,                 // 10
    pub ticket_in_id: i64,             // 8 - Mã giao dịch trạm vào
    pub hub_id: i64,                   // 8
    pub ticket_eTag_id: i64,           // 8
    pub station: i32,                  // 4 - Mã trạm vào
    pub lane: i32,                     // 4 - Mã làn vào
    pub register_vehicle_type: String, // 10
    pub ticket_type: String,           // 1 - L,T,Q,N
    pub seat: i32,                     // 4
    pub weight_goods: i32,             // 4
    pub weight_all: i32,               // 4
    pub vehicle_type: i32,             // 4
    pub status: i32,                   // 4 - 0: thành công
    pub checkin_datetime: i64,         // 8 - epoch (giây)
    pub checkin_commit_datetime: i64,  // 8 - epoch (giây)
    pub general1: [u8; 8],             // 8
    pub general2: [u8; 16],            // 16
}

impl fmt::Debug for VETC_LOOKUP_VEHICLE_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_LOOKUP_VEHICLE_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("request_id", &self.request_id)
            .field("ticket_in_id", &self.ticket_in_id)
            .field("station", &self.station)
            .field("lane", &self.lane)
            .field("plate", &self.plate.trim_end_matches('\0'))
            .field("status", &self.status)
            .field("checkin_datetime", &self.checkin_datetime)
            .finish()
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_CHECKIN_RESERVE_BOO_RESP {
    pub message_length: i32, // 4 bytes - 76
    pub command_id: i32,     // 4 bytes - Mã bản tin: 103
    pub version_id: i32,     // 4 bytes - Phiên bản bản tin
    pub request_id: i64,     // 8 bytes - Long
    pub session_id: i64,     // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi gửi bản tin (millisecond)
    pub process_time: i32, // 4 bytes - Int - Thời gian xử lý tính theo millisecond
    pub ref_trans_id: i64, // 8 bytes - Long - Mã giao dịch unique BOOB
    pub status: i32,    // 4 bytes - Int - Trạng thái: 0: thành công, Còn lại: thất bại
    pub general1: [u8; 8], // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16], // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_CHECKIN_RESERVE_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_CHECKIN_RESERVE_BOO_RESP")
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

#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_COMMIT_BOO {
    pub message_length: i32,     // 4 bytes - 152
    pub command_id: i32,         // 4 bytes - Mã bản tin: 104
    pub version_id: i32,         // 4 bytes - Phiên bản bản tin
    pub request_id: i64,         // 8 bytes - Long
    pub session_id: i64,         // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi gửi bản tin (millisecond)
    pub ticket_id: i64, // 8 bytes - Long - Mã giao dịch BOOA
    pub ref_trans_id: i64, // 8 bytes - Long - Mã giao dịch unique BOOB
    pub tid: String,    // 24 bytes - String - Mã TID
    pub etag: String,   // 24 bytes - String - Mã EPC/ETAG
    pub station: i32,   // 4 bytes - Int - Mã trạm
    pub station_type: String, // 1 byte - String - Trạm kín: C, Trạm mở: O
    pub lane_type: String, // 1 byte - String - Làn Trạm mở: để null; Trạm kín: Làn vào: I (in), làn ra: O (out)
    pub lane: i32,         // 4 bytes - Int - Mã Làn
    pub plate_from_toll: String, // 10 bytes - String - Biển số nhận diện được
    pub commit_datetime: i64, // 8 bytes - Long - Epoch millisecond
    pub general1: [u8; 8], // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16], // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_COMMIT_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_COMMIT_BOO")
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
            .field(
                "plate_from_toll",
                &self.plate_from_toll.trim_end_matches('\0'),
            )
            .field("commit_datetime", &self.commit_datetime)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_COMMIT_BOO_RESP {
    pub message_length: i32, // 4 bytes - 84
    pub command_id: i32,     // 4 bytes - Mã bản tin: 105
    pub version_id: i32,     // 4 bytes - Phiên bản bản tin
    pub request_id: i64,     // 8 bytes - Long
    pub session_id: i64,     // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi phản hồi bản tin (millisecond)
    pub process_time: i32, // 4 bytes - Int - Thời gian xử lý tính theo millisecond
    pub ticket_id: i64, // 8 bytes - Long - Mã giao dịch BOOA
    pub ref_trans_id: i64, // 8 bytes - Long - Mã giao dịch unique BOOB
    pub status: i32,    // 4 bytes - Int - Mã trạng thái, 0: Thành công
    pub general1: [u8; 8], // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16], // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_COMMIT_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_COMMIT_BOO_RESP")
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

/// VETC_CHECKIN_ROLLBACK_BOO - Bản tin checkin rollback
#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_CHECKIN_ROLLBACK_BOO {
    pub message_length: i32,     // 4 bytes - 152
    pub command_id: i32,         // 4 bytes - Mã bản tin: 106
    pub version_id: i32,         // 4 bytes - Phiên bản bản tin
    pub request_id: i64,         // 8 bytes - Long
    pub session_id: i64,         // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi gửi bản tin (millisecond)
    pub ticket_id: i64, // 8 bytes - Long - Mã giao dịch BOOA
    pub ref_trans_id: i64, // 8 bytes - Long - Mã giao dịch unique BOOB
    pub tid: String,    // 24 bytes - String - Mã TID
    pub etag: String,   // 24 bytes - String - Mã EPC/ETAG
    pub station: i32,   // 4 bytes - Int - Mã trạm
    pub station_type: String, // 1 byte - String - Trạm kín: C, Trạm mở: O
    pub lane_type: String, // 1 byte - String - Làn Trạm mở: để null; Trạm kín: Làn vào: I (in), làn ra: O (out)
    pub lane: i32,         // 4 bytes - Int - Mã Làn
    pub plate_from_toll: String, // 10 bytes - String - Biển số nhận diện được
    pub commit_datetime: i64, // 8 bytes - Long - Epoch millisecond
    pub general1: [u8; 8], // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16], // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_CHECKIN_ROLLBACK_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_CHECKIN_ROLLBACK_BOO")
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
            .field(
                "plate_from_toll",
                &self.plate_from_toll.trim_end_matches('\0'),
            )
            .field("commit_datetime", &self.commit_datetime)
            .field("general1", &format_args!("{:?}", &self.general1))
            .field("general2", &format_args!("{:?}", &self.general2))
            .finish()
    }
}

/// VETC_CHECKIN_ROLLBACK_BOO_RESP - Bản tin checkin rollback response
#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_CHECKIN_ROLLBACK_BOO_RESP {
    pub message_length: i32, // 4 bytes - 84
    pub command_id: i32,     // 4 bytes - Mã bản tin: 107
    pub version_id: i32,     // 4 bytes - Phiên bản bản tin
    pub request_id: i64,     // 8 bytes - Long
    pub session_id: i64,     // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi phản hồi bản tin (millisecond)
    pub process_time: i32, // 4 bytes - Int - Thời gian xử lý tính theo millisecond
    pub ticket_id: i64, // 8 bytes - Long - Mã giao dịch BOOA
    pub ref_trans_id: i64, // 8 bytes - Long - Mã giao dịch unique BOOB
    pub status: i32,    // 4 bytes - Int - Mã trạng thái, 0: Thành công
    pub general1: [u8; 8], // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16], // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_CHECKIN_ROLLBACK_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_CHECKIN_ROLLBACK_BOO_RESP")
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

/// RatingDetail - Chi tiết tính phí
/// Bao gồm cả 3 BOOA & BOOB & BOO3
#[allow(dead_code)]
#[derive(Default, Clone)]
pub struct RatingDetail {
    pub price_id: Option<i64>,   // Number - Mã giá
    pub boo: i32,                // Number - Loại BOO
    pub toll_a_id: i32,          // Number - Mã trạm vào
    pub toll_b_id: i32,          // Number - Mã trạm ra
    pub ticket_type: String,     // String - Loại vé
    pub subscription_id: String, // String - Mã vé tháng/quý (nếu có)
    pub price_ticket_type: i32,  // Number - Loại giá vé
    pub price_amount: i32,       // Number - Số tiền
    pub vehicle_type: i32,       // Number - Loại xe
    /// BOT_ID từ RATING_OWNER.BOT_TOLL
    pub bot_id: Option<i64>,
    /// STAGE_ID từ RATING_OWNER.PRICE (P.STAGE_ID)
    pub stage_id: Option<i64>,
}

impl fmt::Debug for RatingDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RatingDetail")
            .field("price_id", &self.price_id)
            .field("boo", &self.boo)
            .field("toll_a_id", &self.toll_a_id)
            .field("toll_b_id", &self.toll_b_id)
            .field("ticket_type", &self.ticket_type.trim_end_matches('\0'))
            .field(
                "subscription_id",
                &self.subscription_id.trim_end_matches('\0'),
            )
            .field("price_ticket_type", &self.price_ticket_type)
            .field("price_amount", &self.price_amount)
            .field("vehicle_type", &self.vehicle_type)
            .field("bot_id", &self.bot_id)
            .field("stage_id", &self.stage_id)
            .finish()
    }
}

/// VETC_CHECKOUT_RESERVE_BOO - Bản tin checkout reserve
/// Size tùy thuộc vào số dòng chi tiết (rating_detail_line)
#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_CHECKOUT_RESERVE_BOO {
    pub message_length: i32,   // 4 bytes - Size tùy thuộc vào số dòng chi tiết
    pub command_id: i32,       // 4 bytes - Mã bản tin: 152
    pub version_id: i32,       // 4 bytes - Phiên bản bản tin
    pub request_id: i64,       // 8 bytes - Long
    pub session_id: i64,       // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi gửi bản tin (millisecond)
    pub tid: String,    // 24 bytes - String - Mã TID
    pub etag: String,   // 24 bytes - String - Mã EPC/ETAG
    pub ticket_in_id: i64, // 8 bytes - Long - Mã giao dịch trạm vào
    pub hub_id: Option<i64>, // 8 bytes - Long - Mã ghi nhận đồng bộ tại hub, mặc định None
    pub ticket_eTag_id: i64, // 8 bytes - Long - Mã giao dịch Boo quản lý thẻ
    pub ticket_out_id: i64, // 8 bytes - Long - Mã giao dịch trạm ra
    pub checkin_datetime: i64, // 8 bytes - Long - Thời gian trạm vào
    pub checkin_commit_datetime: i64, // 8 bytes - Long - Thời gian commit trạm vào
    pub station_in: i32, // 4 bytes - Int - Mã trạm vào
    pub lane_in: i32,   // 4 bytes - Int - Mã làn vào
    pub station_out: i32, // 4 bytes - Int - Mã trạm ra
    pub lane_out: i32,  // 4 bytes - Int - Mã làn ra
    pub plate: String,  // 10 bytes - String - Biển số
    pub ticket_type: String, // 1 byte - String - Loại vé chung
    pub price_ticket_type: i32, // 4 bytes - Int - Loại giá vé chung
    pub trans_amount: i32, // 4 bytes - Int - Số tiền cần trừ phí
    pub trans_datetime: i64, // 8 bytes - Long - epoch datetime, millisecond
    pub rating_detail_line: i32, // 4 bytes - Int - Số dòng tính phí chi tiết; bao gồm cả 3 BOOA & BOOB & BOO3
    pub rating_detail: Vec<RatingDetail>, // Variable - List of RatingDetail - Chi tiết tính phí
    pub general1: [u8; 8],       // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16],      // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_CHECKOUT_RESERVE_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_CHECKOUT_RESERVE_BOO")
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

/// VETC_CHECKOUT_RESERVE_BOO_RESP - Bản tin checkout reserve response
#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_CHECKOUT_RESERVE_BOO_RESP {
    pub message_length: i32, // 4 bytes - 100
    pub command_id: i32,     // 4 bytes - Mã bản tin: 153
    pub version_id: i32,     // 4 bytes - Phiên bản bản tin
    pub request_id: i64,     // 8 bytes - Long
    pub session_id: i64,     // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi gửi bản tin (millisecond)
    pub process_time: i32, // 4 bytes - Int - Thời gian xử lý tính theo millisecond
    pub ticket_in_id: i64, // 8 bytes - Long - Mã giao dịch trạm vào
    pub hub_id: Option<i64>, // 8 bytes - Long - Mã ghi nhận đồng bộ tại hub, mặc định None
    pub ticket_eTag_id: i64, // 8 bytes - Long - Mã giao dịch Boo quản lý thẻ
    pub ticket_out_id: i64, // 8 bytes - Long - Mã giao dịch trạm ra
    pub status: i32, // 4 bytes - Int - Trạng thái: 0: thành công, gọi nhiều lần vẫn trả về thành công, Còn lại: thất bại
    pub general1: [u8; 8], // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16], // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_CHECKOUT_RESERVE_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_CHECKOUT_RESERVE_BOO_RESP")
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

/// VETC_CHECKOUT_COMMIT_BOO - Bản tin checkout commit
#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_CHECKOUT_COMMIT_BOO {
    pub message_length: i32, // 4 bytes - 178
    pub command_id: i32,     // 4 bytes - Mã bản tin: 154
    pub version_id: i32,     // 4 bytes - Phiên bản bản tin
    pub request_id: i64,     // 8 bytes - Long
    pub session_id: i64,     // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi gửi bản tin (millisecond)
    pub tid: String,    // 24 bytes - String - Mã TID
    pub etag: String,   // 24 bytes - String - Mã EPC/ETAG
    pub ticket_in_id: i64, // 8 bytes - Long - Mã giao dịch trạm vào
    pub hub_id: Option<i64>, // 8 bytes - Long - Mã ghi nhận đồng bộ tại hub, mặc định None
    pub ticket_out_id: i64, // 8 bytes - Long - Mã giao dịch trạm ra
    pub ticket_eTag_id: i64, // 8 bytes - Long - Mã giao dịch Boo quản lý thẻ
    pub station_in: i32, // 4 bytes - Int - Mã trạm vào
    pub lane_in: i32,   // 4 bytes - Int - Mã làn vào
    pub station_out: i32, // 4 bytes - Int - Mã trạm ra
    pub lane_out: i32,  // 4 bytes - Int - Mã làn ra
    pub plate: String,  // 10 bytes - String - Biển số
    pub trans_amount: i32, // 4 bytes - Int - Số tiền cần trừ phí
    pub trans_datetime: i64, // 8 bytes - Long - epoch datetime, millisecond
    pub general1: [u8; 8], // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16], // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_CHECKOUT_COMMIT_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_CHECKOUT_COMMIT_BOO")
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

/// VETC_CHECKOUT_COMMIT_BOO_RESP - Bản tin checkout commit response
#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_CHECKOUT_COMMIT_BOO_RESP {
    pub message_length: i32, // 4 bytes - 96
    pub command_id: i32,     // 4 bytes - Mã bản tin: 155
    pub version_id: i32,     // 4 bytes - Phiên bản bản tin
    pub request_id: i64,     // 8 bytes - Long
    pub session_id: i64,     // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi gửi bản tin (millisecond)
    pub ticket_in_id: i64, // 8 bytes - Long - Mã giao dịch trạm vào
    pub hub_id: Option<i64>, // 8 bytes - Long - Mã ghi nhận đồng bộ tại hub, mặc định None
    pub ticket_eTag_id: i64, // 8 bytes - Long - Mã giao dịch Boo quản lý thẻ
    pub ticket_out_id: i64, // 8 bytes - Long - Mã giao dịch trạm ra
    pub status: i32,    // 4 bytes - Int - Mã trạng thái, 0: Thành công
    pub general1: [u8; 8], // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16], // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_CHECKOUT_COMMIT_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_CHECKOUT_COMMIT_BOO_RESP")
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

/// VETC_CHECKOUT_ROLLBACK_BOO - Bản tin checkout rollback
#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_CHECKOUT_ROLLBACK_BOO {
    pub message_length: i32, // 4 bytes - 178
    pub command_id: i32,     // 4 bytes - Mã bản tin: 156
    pub version_id: i32,     // 4 bytes - Phiên bản bản tin
    pub request_id: i64,     // 8 bytes - Long
    pub session_id: i64,     // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi gửi bản tin (millisecond)
    pub tid: String,    // 24 bytes - String - Mã TID
    pub etag: String,   // 24 bytes - String - Mã EPC/ETAG
    pub ticket_in_id: i64, // 8 bytes - Long - Mã giao dịch trạm vào
    pub hub_id: Option<i64>, // 8 bytes - Long - Mã ghi nhận đồng bộ tại hub, mặc định None
    pub ticket_out_id: i64, // 8 bytes - Long - Mã giao dịch trạm ra
    pub ticket_eTag_id: i64, // 8 bytes - Long - Mã giao dịch Boo quản lý thẻ
    pub station_in: i32, // 4 bytes - Int - Mã trạm vào
    pub lane_in: i32,   // 4 bytes - Int - Mã làn vào
    pub station_out: i32, // 4 bytes - Int - Mã trạm ra
    pub lane_out: i32,  // 4 bytes - Int - Mã làn ra
    pub plate: String,  // 10 bytes - String - Biển số
    pub trans_amount: i32, // 4 bytes - Int - Số tiền cần trừ phí
    pub trans_datetime: i64, // 8 bytes - Long - epoch datetime, millisecond
    pub general1: [u8; 8], // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16], // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_CHECKOUT_ROLLBACK_BOO {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_CHECKOUT_ROLLBACK_BOO")
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

/// VETC_CHECKOUT_ROLLBACK_BOO_RESP - Bản tin checkout rollback response
#[allow(dead_code)]
#[derive(Default)]
pub struct VETC_CHECKOUT_ROLLBACK_BOO_RESP {
    pub message_length: i32, // 4 bytes - 96
    pub command_id: i32,     // 4 bytes - Mã bản tin: 157
    pub version_id: i32,     // 4 bytes - Phiên bản bản tin
    pub request_id: i64,     // 8 bytes - Long
    pub session_id: i64,     // 8 bytes - Long
    pub timestamp: i64, // 8 bytes - Long - epoch: thời gian hiện tại khi gửi bản tin (millisecond)
    pub ticket_in_id: i64, // 8 bytes - Long - Mã giao dịch trạm vào
    pub hub_id: Option<i64>, // 8 bytes - Long - Mã ghi nhận đồng bộ tại hub, mặc định None
    pub ticket_eTag_id: i64, // 8 bytes - Long - Mã giao dịch Boo quản lý thẻ
    pub ticket_out_id: i64, // 8 bytes - Long - Mã giao dịch trạm ra
    pub status: i32,    // 4 bytes - Int - Mã trạng thái, 0: Thành công
    pub general1: [u8; 8], // 8 bytes - Unknown - Dùng cho tương lai
    pub general2: [u8; 16], // 16 bytes - Unknown - Dùng cho tương lai
}

impl fmt::Debug for VETC_CHECKOUT_ROLLBACK_BOO_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VETC_CHECKOUT_ROLLBACK_BOO_RESP")
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
