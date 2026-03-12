//! Helper functions: BOO matching, subscription/price validity, white/black/subscription by segment.
//! Time checks (price, subscription) use UTC: check_time from trans_datetime or now_utc_naive();
//! effect_datetime/end_datetime strings from DB are treated as UTC when comparing.

use crate::cache::data::dto::subscription_history_dto::SubscriptionHistoryDto;
use crate::price_ticket_type::value as price_ticket_value;
use crate::utils::now_utc_naive;

/// Entry BOO matches price BOO: None/empty = apply to all BOO.
#[inline]
pub(crate) fn boo_matches(entry_boo: Option<&str>, price_boo: &str) -> bool {
    match entry_boo {
        None => true,
        Some(s) if s.trim().is_empty() => true,
        Some(s) => s.trim().eq_ignore_ascii_case(price_boo.trim()),
    }
}

/// Normalize station pair for subscription stage matching (order does not matter).
#[inline]
pub(crate) fn segment_key(toll_a: i64, toll_b: i64) -> (i64, i64) {
    if toll_a <= toll_b {
        (toll_a, toll_b)
    } else {
        (toll_b, toll_a)
    }
}

/// Check if subscription is valid at fee calculation time (trans_datetime). Comparison uses UTC: trans_datetime or now_utc_naive(); start_date/end_date from DB are treated as UTC.
pub(crate) fn subscription_is_valid_at_time(
    subscription: &SubscriptionHistoryDto,
    trans_datetime: Option<chrono::NaiveDateTime>,
) -> bool {
    let check_time = trans_datetime.unwrap_or_else(now_utc_naive);

    let start_date = match subscription.start_date.as_deref() {
        Some(s) => match chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S") {
            Ok(dt) => dt,
            Err(_) => {
                tracing::warn!(
                    stage_id = subscription.stage_id,
                    start_date = s,
                    "[Processor] Subscription start_date parse failed"
                );
                return false;
            }
        },
        None => {
            tracing::warn!(
                stage_id = subscription.stage_id,
                "[Processor] Subscription start_date is None"
            );
            return false;
        }
    };

    let end_date = subscription
        .end_date
        .as_deref()
        .and_then(|s| chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S").ok());

    let start_valid = start_date <= check_time;
    let end_valid = end_date.map_or(true, |end| end >= check_time);

    start_valid && end_valid
}

/// Parse datetime string with common formats (DB/ODBC may return different). Strings from DB (effect_datetime, end_datetime, start_date, end_date) are treated as UTC when compared to check_time.
fn parse_price_datetime(s: &str) -> Option<chrono::NaiveDateTime> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Strip fractional seconds if present (e.g. "2025-02-28 10:30:00.123456")
    let s = match s.find('.') {
        Some(dot) => s[..dot].trim_end(),
        None => s,
    };
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
    ];
    for fmt in &formats {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt);
        }
        if fmt == &"%Y-%m-%d" {
            if let Ok(d) = chrono::NaiveDate::parse_from_str(s, fmt) {
                return d.and_hms_opt(0, 0, 0);
            }
        }
    }
    None
}

/// Check if price is valid at fee calculation time (trans_datetime). effect_datetime None = no start limit (only check end if present). Comparison uses UTC: trans_datetime or now_utc_naive(); effect/end from DB treated as UTC.
pub(crate) fn price_is_valid_at_time(
    effect_datetime: Option<&str>,
    end_datetime: Option<&str>,
    trans_datetime: Option<chrono::NaiveDateTime>,
) -> bool {
    let check_time = trans_datetime.unwrap_or_else(now_utc_naive);

    let effect_valid = match effect_datetime {
        Some(s) => match parse_price_datetime(s) {
            Some(effect_date) => effect_date <= check_time,
            None => {
                tracing::warn!(
                    effect_datetime = s,
                    "[Processor] Price effect_datetime parse failed (no format matched)"
                );
                return false;
            }
        },
        None => true, // Not specified = no start time limit
    };

    let end_valid = match end_datetime {
        Some(s) => match parse_price_datetime(s) {
            Some(end_date) => end_date >= check_time,
            None => {
                tracing::warn!(
                    end_datetime = s,
                    "[Processor] Price end_datetime parse failed (no format matched)"
                );
                return false;
            }
        },
        None => true,
    };

    if !effect_valid || !end_valid {
        tracing::debug!(
            effect_datetime = ?effect_datetime,
            end_datetime = ?end_datetime,
            check_time = ?check_time,
            effect_valid,
            end_valid,
            "[Processor] Price not valid at trans time"
        );
        return false;
    }
    true
}

/// Returns true if segment (toll_a, toll_b) has at least one station in white list with BOO matching price_boo. WB applies when: nationwide (station_id=0 and (cycle_id=None or cycle_id=0)); or filtered (cycle_id/station_id > 0) when (WB cycle_id = segment cycle_id from TOLL_STAGE) OR (station_id = tollB). On match → create rating detail with 0 amount (create_whitelist_rating_detail).
#[inline]
pub(crate) fn segment_is_whitelisted(
    toll_b: i64,
    segment_cycle_id: Option<i64>,
    white_list: &[(i64, Option<i64>, Option<String>)],
    price_boo: &str,
) -> bool {
    let price_boo = price_boo.trim();
    if white_list.is_empty() {
        return false;
    }
    white_list
        .iter()
        .filter(|(_, _, boo)| boo_matches(boo.as_deref(), price_boo))
        .any(|(tid, entry_cycle_id, _)| {
            let no_cycle_filter = entry_cycle_id.map_or(true, |c| c == 0);
            let is_nationwide = *tid == 0 && no_cycle_filter;
            let has_filter = *tid > 0 || entry_cycle_id.map(|c| c > 0).unwrap_or(false);
            let segment_ok = (*tid == toll_b)
                || (entry_cycle_id.is_some()
                    && entry_cycle_id.map(|c| c > 0).unwrap_or(false)
                    && segment_cycle_id == *entry_cycle_id);
            is_nationwide || (has_filter && segment_ok)
        })
}

/// Returns true if segment (toll_a, toll_b) has at least one station in black list with BOO matching price_boo. Same logic as white list: nationwide (station_id=0, cycle_id=None or 0) or (cycle_id/station_id > 0 and (cycle_id matches segment or station_id = tollB)).
#[inline]
pub(crate) fn segment_in_black_list(
    toll_b: i64,
    segment_cycle_id: Option<i64>,
    black_list: &[(i64, Option<i64>, Option<String>)],
    price_boo: &str,
) -> bool {
    let price_boo = price_boo.trim();
    if black_list.is_empty() {
        return false;
    }
    black_list
        .iter()
        .filter(|(_, _, boo)| boo_matches(boo.as_deref(), price_boo))
        .any(|(tid, entry_cycle_id, _)| {
            let no_cycle_filter = entry_cycle_id.map_or(true, |c| c == 0);
            let is_nationwide = *tid == 0 && no_cycle_filter;
            let has_filter = *tid > 0 || entry_cycle_id.map(|c| c > 0).unwrap_or(false);
            let segment_ok = (*tid == toll_b)
                || (entry_cycle_id.is_some()
                    && entry_cycle_id.map(|c| c > 0).unwrap_or(false)
                    && segment_cycle_id == *entry_cycle_id);
            is_nationwide || (has_filter && segment_ok)
        })
}

/// price_ticket_type for WB (whitelist): cycle_id None or 0 → WL_ALL, else → WL_TOLL.
#[inline]
pub(crate) fn price_ticket_type_for_wb(cycle_id: Option<i64>) -> i32 {
    match cycle_id {
        None => price_ticket_value::WL_ALL,
        Some(0) => price_ticket_value::WL_ALL,
        Some(_) => price_ticket_value::WL_TOLL,
    }
}

/// price_ticket_type for BL (black list): cycle_id None or 0 → BL_ALL, else → BL_TOLL.
#[inline]
pub(crate) fn price_ticket_type_for_bl(cycle_id: Option<i64>) -> i32 {
    match cycle_id {
        None => price_ticket_value::BL_ALL,
        Some(0) => price_ticket_value::BL_ALL,
        Some(_) => price_ticket_value::BL_TOLL,
    }
}

/// Returns true if segment (toll_a, toll_b) is covered by subscription.
#[inline]
pub(crate) fn segment_covered_by_subscription(
    toll_a: i64,
    toll_b: i64,
    subscription_stages: &std::collections::HashSet<(i64, i64)>,
) -> bool {
    subscription_stages.contains(&segment_key(toll_a, toll_b))
}

/// Returns subscription entry matching stage_id, BOO and validity time. Used when segment is covered by subscription to get price_id/bot_id if present.
pub(crate) fn get_subscription_entry_for_stage<'a>(
    stage_id: i64,
    boo_str: &str,
    subscription_history: Option<&'a [SubscriptionHistoryDto]>,
    trans_datetime: Option<chrono::NaiveDateTime>,
) -> Option<&'a SubscriptionHistoryDto> {
    subscription_history.and_then(|hist| {
        hist.iter().find(|sub| {
            sub.stage_id == stage_id
                && boo_matches(sub.stage_boo.as_deref(), boo_str)
                && subscription_is_valid_at_time(sub, trans_datetime)
        })
    })
}

/// Get price_type from subscription history for segment with matching stage_id and BOO.
pub(crate) fn get_subscription_price_type(
    toll_a: i64,
    toll_b: i64,
    stage_id: Option<i64>,
    boo_str: &str,
    subscription_history: Option<&[SubscriptionHistoryDto]>,
    trans_datetime: Option<chrono::NaiveDateTime>,
) -> String {
    if let Some(sub_history) = subscription_history {
        if let Some(sid) = stage_id {
            if let Some(sub) = sub_history.iter().find(|sub| {
                sub.stage_id == sid
                    && boo_matches(sub.stage_boo.as_deref(), boo_str)
                    && subscription_is_valid_at_time(sub, trans_datetime)
            }) {
                return sub.price_type.as_deref().unwrap_or("T").to_string();
            }
        }
    }
    "T".to_string()
}

/// Closed station + nationwide: TYPE_STATION in ("0", "2"). None (legacy cache) = accept for backward compatibility.
#[inline]
fn type_station_closed(ts: Option<&String>) -> bool {
    match ts.and_then(|s| s.trim().get(..1)) {
        None => true,
        Some("0") | Some("2") => true,
        _ => false,
    }
}

/// Open station + nationwide: TYPE_STATION in ("0", "1"). None (legacy cache) = accept for backward compatibility.
#[inline]
pub(crate) fn type_station_open(ts: Option<&String>) -> bool {
    match ts.and_then(|s| s.trim().get(..1)) {
        None => true,
        Some("0") | Some("1") => true,
        _ => false,
    }
}

/// Get vehicle_type from EX_LIST for open station (check-in): only entries TYPE_STATION 0/1; prefer matching station_id, then 0 (nationwide). Java: getFirstElement(stationId|vehicleId, getCacheExList()).
pub(crate) fn vehicle_type_from_ex_list_open(
    ex_list: &[(i64, Option<String>, String, Option<String>)],
    station_id: i64,
) -> Option<String> {
    let mut nationwide: Option<String> = None;
    for (sid, _, vt, ts) in ex_list.iter() {
        if !type_station_open(ts.as_ref()) {
            continue;
        }
        if *sid == station_id {
            return Some(vt.clone());
        }
        if *sid == 0 {
            nationwide = nationwide.or_else(|| Some(vt.clone()));
        }
    }
    nationwide
}

/// Effective vehicle_type for segment (closed station) by BOO: prefer ex_list, only entries TYPE_STATION 0/2; match station_id = toll_a/toll_b/0. Single pass.
pub(crate) fn effective_vehicle_type(
    toll_a: i64,
    toll_b: i64,
    ex_list: &[(i64, Option<String>, String, Option<String>)],
    price_boo: &str,
    default: &str,
) -> String {
    let price_boo = price_boo.trim();
    let mut nationwide: Option<String> = None;
    for (sid, boo, vt, ts) in ex_list.iter() {
        if !boo_matches(boo.as_deref(), price_boo) || !type_station_closed(ts.as_ref()) {
            continue;
        }
        if *sid == toll_a || *sid == toll_b {
            return vt.clone();
        }
        if *sid == 0 {
            nationwide = nationwide.or_else(|| Some(vt.clone()));
        }
    }
    nationwide.unwrap_or_else(|| default.to_string())
}

/// Effective ticket_type for segment (closed station): prefer ex_price_list; only TYPE_STATION 0/2; match station_id = toll_a/toll_b/0. Single pass.
pub(crate) fn effective_ticket_type(
    toll_a: i64,
    toll_b: i64,
    ex_price_list: &[(i64, Option<String>, i32, Option<String>)],
    price_boo: &str,
    default: &str,
) -> String {
    let price_boo = price_boo.trim();
    let mut nationwide: Option<String> = None;
    for (sid, boo, pt, ts) in ex_price_list.iter() {
        if !boo_matches(boo.as_deref(), price_boo) || !type_station_closed(ts.as_ref()) {
            continue;
        }
        if *sid == toll_a || *sid == toll_b {
            return pt.to_string();
        }
        if *sid == 0 {
            nationwide = nationwide.or_else(|| Some(pt.to_string()));
        }
    }
    nationwide.unwrap_or_else(|| default.to_string())
}
