//! Segment-based logic: get price by stage_id, get price with ex (no-route), BOO from TOLL, subscription/price helpers.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures::future::join_all;

use r2d2::Pool;

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::data::closed_cycle_transition_stage_cache::get_segment_cycle_id;
use crate::cache::data::dto::price_dto::PriceDto;
use crate::cache::data::dto::subscription_history_dto::SubscriptionHistoryDto;
use crate::cache::data::price_cache::get_price;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::models::TollCache::get_toll_with_fallback;

use super::error::TollFeeError;
use super::helpers::{
    boo_matches, effective_vehicle_type, price_is_valid_at_time, segment_covered_by_subscription,
    subscription_is_valid_at_time,
};
use super::types::ParsedBoo;

/// Compute amount from price if price is valid at trans_datetime.
pub(crate) fn calculate_amount_from_price(
    price: &PriceDto,
    trans_datetime: Option<chrono::NaiveDateTime>,
) -> i32 {
    let price_valid = price_is_valid_at_time(
        price.effect_datetime.as_deref(),
        price.end_datetime.as_deref(),
        trans_datetime,
    );
    if price_valid {
        price.price_amount.unwrap_or(0) as i32
    } else {
        0
    }
}

/// Check subscription by stage_id with BOO and time.
pub(crate) fn is_subscribed_by_stage_id(
    stage_id: Option<i64>,
    boo_str: &str,
    subscription_history: Option<&[SubscriptionHistoryDto]>,
    subscription_stage_ids: Option<&HashSet<i64>>,
    trans_datetime: Option<chrono::NaiveDateTime>,
) -> bool {
    if let Some(sub_history) = subscription_history {
        if let Some(sid) = stage_id {
            return sub_history.iter().any(|sub| {
                sub.stage_id == sid
                    && boo_matches(sub.stage_boo.as_deref(), boo_str)
                    && subscription_is_valid_at_time(sub, trans_datetime)
            });
        }
    }
    stage_id
        .and_then(|sid| subscription_stage_ids.map(|set| set.contains(&sid)))
        .unwrap_or(false)
}

/// Check subscription for segment (toll_a, toll_b) with BOO.
pub(crate) fn check_segment_subscription(
    toll_a: i64,
    toll_b: i64,
    boo_str: &str,
    subscription_set: &HashSet<(i64, i64)>,
    subscription_history: Option<&[SubscriptionHistoryDto]>,
    price: Option<&PriceDto>,
    trans_datetime: Option<chrono::NaiveDateTime>,
) -> bool {
    if segment_covered_by_subscription(toll_a, toll_b, subscription_set) {
        return true;
    }
    if let Some(p) = price {
        if let Some(sid) = p.stage_id {
            if let Some(sub_history) = subscription_history {
                return sub_history.iter().any(|sub| {
                    sub.stage_id == sid
                        && boo_matches(sub.stage_boo.as_deref(), boo_str)
                        && subscription_is_valid_at_time(sub, trans_datetime)
                });
            }
        }
    }
    false
}

/// Get price for segment with ex_list/ex_price_list (ex_list has type_station for closed station filter 0/2). Used when segment is not in CLOSED_CYCLE_TRANSITION_STAGE (resolve_direct_price_opt, no-route).
pub(crate) async fn get_segment_price_with_ex(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
    toll_a: i64,
    toll_b: i64,
    vehicle_type: &str,
    ticket_type: &str,
    ex_list: &[(i64, Option<String>, String, Option<String>)],
    ex_price_list: &[(i64, Option<String>, i32, Option<String>)],
    boo_str: &str,
) -> Option<PriceDto> {
    let eff_vehicle = effective_vehicle_type(toll_a, toll_b, ex_list, boo_str, vehicle_type);
    get_price(db_pool, cache, toll_a, toll_b, &eff_vehicle).await
}

/// Get BOO for multiple stations in parallel (used in direct price segment infos).
pub(crate) async fn get_boo_map_for_tolls(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
    toll_ids: &[i64],
) -> HashMap<i64, ParsedBoo> {
    if toll_ids.is_empty() {
        return HashMap::new();
    }
    let futures: Vec<_> = toll_ids
        .iter()
        .map(|&toll_id| get_boo_from_toll_by_sout(db_pool.clone(), cache.clone(), toll_id))
        .collect();
    let results = join_all(futures).await;
    toll_ids
        .iter()
        .zip(results)
        .map(|(&toll_id, res)| (toll_id, res.unwrap_or_else(|_| ParsedBoo::default())))
        .collect()
}

/// Get BOO from TOLL table by toll_id (exit station sout).
pub(crate) async fn get_boo_from_toll_by_sout(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
    toll_id: i64,
) -> Result<ParsedBoo, TollFeeError> {
    let tid = i32::try_from(toll_id).map_err(|_| {
        tracing::warn!(toll_id, "[Processor] TollFee toll_id out of i32 range");
        TollFeeError::BooNotFoundForToll { toll_id }
    })?;
    let toll = match get_toll_with_fallback(tid, db_pool, cache).await {
        Some(t) => t,
        None => {
            tracing::warn!(toll_id, "[Processor] TollFee TOLL not found for toll");
            return Err(TollFeeError::BooNotFoundForToll { toll_id });
        }
    };
    let boo_str = toll.boo.trim();
    if boo_str.is_empty() {
        tracing::warn!(toll_id, "[Processor] TollFee TOLL.boo empty");
        return Err(TollFeeError::BooNotFoundForToll { toll_id });
    }
    Ok(ParsedBoo::from_str(boo_str))
}
