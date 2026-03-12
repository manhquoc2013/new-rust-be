//! Tiện ích: format_bytes_hex, wrap_encrypted_reply (length prefix 4 bytes LE), format_boo_request_id, parse_env_bool_loose.

use crate::constants::{bect, fe};

/// Parse giá trị env "1"|"true"|"yes" (không phân biệt hoa thường) thành true; còn lại false. Dùng chung cho CONNECT_BYPASS_AUTH, BECT_SKIP_*.
#[inline]
pub fn parse_env_bool_loose(s: Option<&str>) -> bool {
    s.map(|s| matches!(s.trim().to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Chuỗi etag đã trim null cuối và space hai đầu (dùng cho cache/fee, commit).
#[inline]
pub fn normalize_etag(s: &str) -> String {
    s.trim_end_matches('\0').trim().to_string()
}

/// Format bytes thành hex string để log.
/// Hiển thị tối đa `max_bytes` byte đầu, nếu dài hơn thêm "... (N bytes total)".
#[allow(dead_code)]
pub fn format_bytes_hex(data: &[u8], max_bytes: usize) -> String {
    let display_len = data.len().min(max_bytes);
    let hex_str: Vec<String> = data[..display_len]
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect();
    let mut result = hex_str.join(" ");
    if data.len() > max_bytes {
        result.push_str(&format!(" ... ({} bytes total)", data.len()));
    }
    result
}

/// Đọc env BECT_SKIP_ACCOUNT_CHECK: "1"|"true"|"yes" => bỏ qua kiểm tra subscriber/account. Dùng chung checkin + commit BECT.
#[inline]
pub fn bect_skip_account_check() -> bool {
    parse_env_bool_loose(std::env::var("BECT_SKIP_ACCOUNT_CHECK").ok().as_deref())
}

/// Trả về chuỗi mô tả ngắn cho mã lỗi FE/BECT để ghi log (trace failed transaction).
#[inline]
pub fn fe_status_detail(status: i32) -> &'static str {
    match status {
        fe::NOT_FOUND_STATION_LANE => "NOT_FOUND_STATION_LANE",
        fe::NOT_FOUND_STAGE => "NOT_FOUND_STAGE",
        fe::NOT_FOUND_PRICE_INFO => "NOT_FOUND_PRICE_INFO",
        fe::NOT_FOUND_ROUTE_TRANSACTION => "NOT_FOUND_ROUTE_TRANSACTION",
        fe::NOT_FOUND_TOLL_TRANSACTION => "NOT_FOUND_TOLL_TRANSACTION",
        fe::NOT_FOUND_LANE_TRANSACTION => "NOT_FOUND_LANE_TRANSACTION",
        fe::SAVE_DB_ERROR => "SAVE_DB_ERROR",
        fe::VERIFY_DB_FAILED => "VERIFY_DB_FAILED",
        fe::DUPLICATE_TRANSACTION => "DUPLICATE_TRANSACTION",
        fe::BOO_1_ERROR => "BOO_1_ERROR",
        fe::BOO_2_ERROR => "BOO_2_ERROR",
        fe::ERROR_REASON_SYSTEM => "SYSTEM_ERROR",
        fe::USER_NOT_FOUND => "USER_NOT_FOUND",
        fe::ACCOUNT_NOT_ACTIVATED => "ACCOUNT_NOT_ACTIVATED",
        bect::SUBSCRIBER_NOT_FOUND => "SUBSCRIBER_NOT_FOUND",
        bect::SUBSCRIBER_IS_NOT_ACTIVE => "SUBSCRIBER_IS_NOT_ACTIVE",
        bect::ACCOUNT_IS_NOT_ACTIVE => "ACCOUNT_IS_NOT_ACTIVE",
        bect::ACCOUNT_NOT_ENOUGH_MONEY => "ACCOUNT_NOT_ENOUGH_MONEY",
        bect::ACCOUNT_NOT_FOUND => "ACCOUNT_NOT_FOUND",
        bect::DEBIT_AMOUNT_IS_OVER => "DEBIT_AMOUNT_IS_OVER",
        bect::DEBIT_IS_OVERTIME => "DEBIT_IS_OVERTIME",
        _ => "UNKNOWN_STATUS",
    }
}

/// Bọc encrypted payload với length prefix (4 bytes LE) theo protocol FE.
/// Trả về `[u32_le(total_len), ...encrypted]` với total_len = encrypted.len() + 4.
pub fn wrap_encrypted_reply(encrypted: Vec<u8>) -> Vec<u8> {
    let total_len = (encrypted.len() + 4) as u32;
    let mut out = Vec::with_capacity(4 + encrypted.len());
    out.extend_from_slice(&total_len.to_le_bytes());
    out.extend_from_slice(&encrypted);
    out
}

/// Build trace prefix for log message: "[transport_trans_id|ticket_id] [etag] [request_id]".
/// Each value is wrapped in []. Use "-" when not available. Use in message content so fields appear in log body.
#[inline]
pub fn log_trace_ids(
    request_id: i64,
    transport_trans_id_or_ticket_id: Option<i64>,
    etag: Option<&str>,
) -> String {
    let id = transport_trans_id_or_ticket_id
        .map(|x| x.to_string())
        .unwrap_or_else(|| "-".to_string());
    let etag_s = etag
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("-");
    let rid = request_id.to_string();
    format!("[{}] [{}] [{}]", id, etag_s, rid)
}
