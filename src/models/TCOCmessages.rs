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
    /// Protocol version (4 bytes).
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
    /// Protocol version (4 bytes).
    pub version_id: i32,
    pub request_id: i64,
    pub session_id: i64,
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
    pub request_id: i64,
    pub session_id: i64,
}
impl fmt::Debug for FE_SHAKE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_SHAKE")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .finish()
    }
}
#[derive(Default)]
pub struct FE_SHAKE_RESP {
    pub message_length: i32,
    pub command_id: i32,
    pub request_id: i64,
    pub session_id: i64,
    pub status: i32,
}

impl fmt::Debug for FE_SHAKE_RESP {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FE_SHAKE_RESP")
            .field("message_length", &self.message_length)
            .field("command_id", &format_args!("0x{:02X}", self.command_id))
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

    /// Thông tin EPC của eTag, 24 bytes.
    pub etag: String,
    /// Mã trạm thu phí, 4 bytes.
    pub station: i32,
    /// Mã làn xe đi qua (tùy chọn), 4 bytes.
    pub lane: i32,
    /// Biển số xe do FE ghi nhận (tùy chọn), 10 bytes.
    pub plate: String,
    /// Mã giao dịch từ đầu đọc thẻ (tùy chọn), 24 bytes.
    pub tid: String,
    /// Giá trị bảo mật của thẻ eTag (tùy chọn), 16 bytes.
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

    /// Mã trạng thái giao dịch. 0 là thành công.
    pub status: i32,
    /// Etag của phương tiện, 24 bytes.
    pub etag: String,
    /// Mã trạm, 4 bytes.
    pub station: i32,
    /// Mã làn, 4 bytes.
    pub lane: i32,
    /// Mã vé của giao dịch do Back End tạo ra, 8 bytes (Long).
    pub ticket_id: i64,
    /// Loại vé (tháng, lượt, quý, năm), 4 bytes.
    pub ticket_type: i32,
    /// Giá cước sẽ bị trừ, 4 bytes.
    pub price: i32,
    /// Loại phương tiện, 4 bytes.
    pub vehicle_type: i32,
    /// Biển số xe, 10 bytes.
    pub plate: String,
    /// Loại biển số (trắng, xanh, đỏ...), 4 bytes.
    pub plate_type: i32,
    /// ID của loại giá vé miễn giảm, 4 bytes.
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

/// FE COMMIT (4.2.3.9) – Bản tin Front End gửi khi muốn thực hiện trừ tiền tài khoản xe qua trạm.
///
/// **REQUEST HEADER**
/// - Message Length = 98 (fix)
/// - Command Id: 0x06
/// - Request Id: Front End tạo duy nhất và truyền đến (Long, 8 bytes)
/// - Session Id: Session phiên đăng nhập nhận được từ CONNECT RESP (Long, 8 bytes)
///
/// **REQUEST BODY** (layout bytes, theo spec)
///
/// | Field             | Bytes | Type   | Mandatory | Description |
/// |-------------------|-------|--------|-----------|-------------|
/// | Message Length    | 4     | Int    | M         | Kích thước bản tin: Fix = 98 bytes |
/// | Command Id        | 4     | Int    | M         | 0x06 |
/// | Request Id        | 8     | Long   | M         | Thông tin duy nhất FE gửi đến |
/// | Session Id        | 8     | Long   | M         | Session phiên đăng nhập, do BE trả về |
/// | Etag              | 24    | String | M         | Thông tin EPC của etag của phương tiện |
/// | Station           | 4     | Int    | M         | Mã trạm thu phí đang xử lý |
/// | Lane              | 4     | Int    | O         | Mã làn xe đi qua; 0 nếu chưa xác định (cổng long môn xa) |
/// | Ticket Id         | 8     | Long   | M         | Mã vé giao dịch FE nhận từ BE khi Reserve (CHECKIN RESP) |
/// | Status            | 4     | Int    | M         | Trạng thái khớp biển số (so sánh ảnh) |
/// | Plate             | 10    | String | O         | Biển số thực tế camera nhận được; có thể rỗng |
/// | Image Count       | 4     | Int    | O         | Số lượng ảnh liên quan. Tên ảnh: TicketId_0x (X = Image Count) |
/// | Vehicle Length    | 4     | Int    | O         | Chiều dài phương tiện (cm); 0 nếu không xác định (xe Rơ Mooc 20/40 feet) |
/// | Transaction Amount| 4     | Int    | O         | Giá tiền thực tế FE tính. =0: BE trừ theo CHECKIN_RESP; ≠0: trừ theo số này |
/// | Weight            | 4     | Int    | O         | Cân nặng trọng tải xe; 0 nếu chưa xác định |
/// | Reason ID         | 4     | Int    | O         | Lý do commit/rollback, trạm đặc biệt; mặc định 0 |
///
/// **Tổng: 98 bytes** (4+4+8+8+24+4+4+8+4+10+4+4+4+4+4).
#[derive(Default)]
pub struct FE_COMMIT_IN {
    pub message_length: i32,
    pub command_id: i32,
    pub request_id: i64,
    pub session_id: i64,

    /// Etag: thông tin EPC của phương tiện, 24 bytes. (M)
    pub etag: String,
    /// Station: mã trạm thu phí đang xử lý, 4 bytes. (M)
    pub station: i32,
    /// Lane: làn thực tế khi xe đi qua (hậu kiểm, đảo làn/cổng xa), 4 bytes. (O) 0 nếu chưa xác định.
    pub lane: i32,
    /// Ticket Id: id giao dịch từ CHECKIN RESP, 8 bytes (Long). (M)
    pub ticket_id: i64,
    /// Status: trạng thái so sánh ảnh biển số khớp/không khớp, 4 bytes. (M)
    pub status: i32,
    /// Plate: biển số thực tế camera nhận được, 10 bytes. (O) Có thể rỗng.
    pub plate: String,
    /// Image Count: số lượng ảnh liên quan (tên ảnh TicketId_0x), 4 bytes. (O)
    pub image_count: i32,
    /// Vehicle Length: chiều dài phương tiện (cm), 4 bytes. (O) 0 nếu không xác định.
    pub vehicle_length: i32,
    /// Transaction Amount: giá tiền thực tế FE tính; 0 = BE trừ theo CHECKIN_RESP, 4 bytes. (O)
    pub transaction_amount: i32,
    /// Weight: cân nặng trọng tải xe, 4 bytes. (O) 0 nếu chưa xác định.
    pub weight: i32,
    /// Reason ID: lý do commit/rollback, trạm đặc biệt; mặc định 0, 4 bytes. (O)
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
    /// Trạng thái so sánh biển số (0: không khớp, 1: khớp, 2: không nhận dạng được), 4 bytes.
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

    /// Thông tin EPC của eTag, 24 bytes.
    pub etag: String,
    /// Mã trạm thu phí, 4 bytes.
    pub station: i32,
    /// Mã làn xe đi qua (tùy chọn), 4 bytes. Một số trạm mà cổng long môn đặt xa, có thể chưa xác định được chính xác mã làn khi xe vào trạm thì sẽ nhập vào là 0.
    pub lane: i32,
    /// Mã vé giao dịch Front End nhận được từ Back End khi Checkin, 8 bytes (Long).
    pub ticket_id: i64,
    /// Trạng thái khớp biển số, 4 bytes.
    pub status: i32,
    /// Biển số thực tế (tùy chọn), 10 bytes. Có thể là NULL nếu Front End không nhận dạng được.
    pub plate: String,
    /// Số ảnh chụp phương tiện Front End chụp được (tùy chọn), 4 bytes. Tên ảnh sẽ là Ticket Id_0x (trong đó X = Image Count).
    pub image_count: i32,
    /// Cân nặng trọng tải xe (tùy chọn), 4 bytes. Nếu chưa xác định được cân nặng thì nhập vào là 0.
    pub weight: i32,
    /// Lý do commit/rollback, phục vụ cho một số trạm đặc biệt (tùy chọn), 4 bytes. Mặc định là 0.
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
    /// Trạng thái rollback (0: thành công, khác 0: thất bại), 4 bytes.
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
    /// Trạng thái terminate (0: thành công, khác 0: thất bại), 4 bytes.
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
