//! Chuyển epoch (giây hoặc millisecond) sang DateTime; timestamp ms và chuỗi thời gian DB.
//! Mọi thời điểm "hiện tại" dùng UTC để thống nhất với kiểm tra hiệu lực bảng giá/subscription.

use chrono::{DateTime, Utc};

use crate::constants::epoch;

/// Chuyển epoch (i64) sang `Option<DateTime<Utc>>`.
/// Tự nhận dạng: nếu `epoch >= 1_000_000_000_000` coi là **millisecond**, ngược lại coi là **giây**.
/// Trả về `None` nếu `epoch <= 0` hoặc giá trị không hợp lệ.
#[inline]
pub fn epoch_to_datetime_utc(epoch: i64) -> Option<DateTime<Utc>> {
    if epoch <= 0 {
        return None;
    }
    if epoch >= epoch::MS_THRESHOLD {
        DateTime::from_timestamp_millis(epoch)
    } else {
        DateTime::from_timestamp(epoch, 0)
    }
}

/// Trả về Unix timestamp hiện tại (millisecond) dạng i64.
#[inline]
pub fn timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before UNIX_EPOCH")
        .as_millis() as i64
}

/// Trả về thời gian UTC hiện tại dạng chuỗi "%Y-%m-%d %H:%M:%S" (dùng cho DB datetime).
#[inline]
pub fn now_utc_db_string() -> String {
    Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Trả về thời gian UTC hiện tại dạng NaiveDateTime (dùng khi kiểm tra hiệu lực bảng giá/subscription).
#[inline]
pub fn now_utc_naive() -> chrono::NaiveDateTime {
    Utc::now().naive_utc()
}

/// Format `DateTime<Utc>` sang chuỗi DB "%Y-%m-%d %H:%M:%S" (dùng chung, tránh duplicate format string).
#[inline]
pub fn format_datetime_utc_db(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Chuyển epoch millisecond sang chuỗi DB "%Y-%m-%d %H:%M:%S" (UTC). Trả về `None` nếu epoch không hợp lệ.
#[allow(dead_code)]
pub fn epoch_ms_to_db_datetime_string(ms: i64) -> Option<String> {
    use chrono::TimeZone;
    let secs = ms / 1000;
    let nsecs = ((ms % 1000).unsigned_abs() as u32) * 1_000_000;
    Utc.timestamp_opt(secs, nsecs)
        .single()
        .as_ref()
        .map(format_datetime_utc_db)
}

/// Parse chuỗi datetime DB "%Y-%m-%d %H:%M:%S" sang epoch milliseconds. Trả về `None` nếu rỗng hoặc parse lỗi.
#[allow(dead_code)]
pub fn db_datetime_string_to_epoch_ms(s: Option<&str>) -> Option<i64> {
    let s = s.as_ref().map(|x| x.trim()).filter(|x| !x.is_empty())?;
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|dt| dt.and_utc().timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_seconds() {
        // 2024-02-14 00:00:00 UTC = 1_707_782_400 sec
        let dt = epoch_to_datetime_utc(1_707_782_400).unwrap();
        assert_eq!(dt.timestamp(), 1_707_782_400);
        assert_eq!(dt.timestamp_millis(), 1_707_782_400_000);
    }

    #[test]
    fn test_epoch_milliseconds() {
        // 2024-02-14 00:00:00 UTC = 1_707_782_400_000 ms
        let dt = epoch_to_datetime_utc(1_707_782_400_000).unwrap();
        assert_eq!(dt.timestamp(), 1_707_782_400);
        assert_eq!(dt.timestamp_millis(), 1_707_782_400_000);
    }

    #[test]
    fn test_epoch_zero_or_negative() {
        assert!(epoch_to_datetime_utc(0).is_none());
        assert!(epoch_to_datetime_utc(-1).is_none());
    }

    #[test]
    fn test_threshold() {
        // 1e12 - 1 = 999_999_999_999 coi là giây
        let dt = epoch_to_datetime_utc(999_999_999_999).unwrap();
        assert_eq!(dt.timestamp(), 999_999_999_999);
        // 1e12 coi là ms
        let dt = epoch_to_datetime_utc(1_000_000_000_000).unwrap();
        assert_eq!(dt.timestamp_millis(), 1_000_000_000_000);
    }
}
