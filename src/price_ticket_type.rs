//! Định nghĩa `price_ticket_type` theo Java TCOCConstants.
//! DB lưu dạng chuỗi (BL, W, 1, 94, ...); DTO / logic dùng kiểu số (i32).
//!
//! **Chuẩn load (chuỗi → i32):** chỉ dùng `from_db_string(Option<&str>)` hoặc `from_str(&str)`.
//! **Chuẩn ghi (i32 → chuỗi DB):** chỉ dùng `to_db_string(i32)`.
//!
//! Hằng số value/db_code nằm trong `crate::constants::price_ticket_type`; module này re-export và cung cấp hàm chuyển đổi.

pub use crate::constants::price_ticket_type::{db_code, value};

/// Convert from DB string to numeric value (DTO / logic).
/// Chấp nhận: số "1","93"-"99"; code "BL","BL_ALL","W","W_ALL","99",...
/// Trả về `value::DEFAULT` (1) nếu không parse được.
#[inline]
pub fn from_db_string(s: Option<&str>) -> i32 {
    let s = match s {
        Some(x) => x.trim(),
        None => return value::DEFAULT,
    };
    if s.is_empty() {
        return value::DEFAULT;
    }
    // Số trực tiếp
    if let Ok(n) = s.parse::<i32>() {
        if (1..=99).contains(&n) {
            return n;
        }
    }
    // Code chuỗi (không phân biệt hoa thường)
    match s.to_uppercase().as_str() {
        "BL" => value::BL_TOLL,
        "BL_ALL" => value::BL_ALL,
        "W" | "WL" | "WL_TOLL" => value::WL_TOLL,
        "W_ALL" | "WL_ALL" => value::WL_ALL,
        "FREE_TURNING" | "99" => value::FREE_TURNING,
        "93" | "LP_VD3" | "LP_IN_VD3" => value::LP_IN_VD3,
        "96" | "FREE_TURNING_SECOND" => value::FREE_TURNING_SECOND,
        _ => value::DEFAULT,
    }
}

/// Chuẩn load khi có chuỗi (trim + from_db_string). Dùng thống nhất mọi nơi đọc price_ticket_type từ DB/cache/DTO.
#[inline]
pub fn from_str(s: &str) -> i32 {
    from_db_string(Some(s.trim()))
}

/// Chuyển từ giá trị số (DTO) sang chuỗi ghi DB
#[inline]
pub fn to_db_string(n: i32) -> String {
    match n {
        value::DEFAULT => db_code::DEFAULT.to_string(),
        value::LP_IN_VD3 => db_code::LP_IN_VD3.to_string(),
        value::BL_TOLL => db_code::BL_TOLL.to_string(),
        value::BL_ALL => db_code::BL_ALL.to_string(),
        value::FREE_TURNING_SECOND => db_code::FREE_TURNING_SECOND.to_string(),
        value::WL_TOLL => db_code::WL_TOLL.to_string(),
        value::WL_ALL => db_code::WL_ALL.to_string(),
        value::FREE_TURNING => db_code::FREE_TURNING.to_string(),
        _ => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_db_accepts_numeric() {
        assert_eq!(from_db_string(Some("1")), 1);
        assert_eq!(from_db_string(Some("94")), 94);
        assert_eq!(from_db_string(Some("97")), 97);
    }

    #[test]
    fn from_db_accepts_codes() {
        assert_eq!(from_db_string(Some("BL")), 94);
        assert_eq!(from_db_string(Some("W")), 97);
        assert_eq!(from_db_string(Some("  bl  ")), 94);
    }

    #[test]
    fn to_db_returns_code() {
        assert_eq!(to_db_string(94), "BL");
        assert_eq!(to_db_string(97), "W");
        assert_eq!(to_db_string(1), "1");
    }

    #[test]
    fn from_str_same_as_from_db_string() {
        assert_eq!(from_str("BL"), from_db_string(Some("BL")));
        assert_eq!(from_str("  1  "), from_db_string(Some("1")));
    }
}
