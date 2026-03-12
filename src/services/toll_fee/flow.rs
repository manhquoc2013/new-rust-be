//! Fee calculation flow: resolve direct price, same station, segment by stage_id, no route.

use futures::future::join_all;

use crate::cache::data::closed_cycle_transition_stage_cache::{
    get_segment_cycle_id, get_stage_id_for_segment, get_toll_a_toll_b_for_stage_id,
};
use crate::cache::data::dto::price_dto::PriceDto;
use crate::cache::data::price_cache::{get_price, get_price_by_stage_id};
use crate::models::RatingDetail;

use super::detail::{
    create_regular_rating_detail, create_subscription_rating_detail, create_whitelist_rating_detail,
};
use super::direct_price::build_same_station_detail_for_direct_price;
use super::error::TollFeeError;
use super::helpers::{
    effective_vehicle_type, get_subscription_entry_for_stage, price_is_valid_at_time,
    price_ticket_type_for_bl, segment_covered_by_subscription, segment_in_black_list,
    segment_is_whitelisted,
};
use super::segment::{
    calculate_amount_from_price, get_boo_from_toll_by_sout, get_segment_price_with_ex,
    is_subscribed_by_stage_id,
};
use super::types::{FeeCalculationContext, ParsedBoo, RatingDetailBuilder};

/// Resolve direct price for station pair (entry, exit). With list_ctx: black list overrides ex list — if (sin, sout) in black list use raw get_price, else get_segment_price_with_ex.
/// Fetches BOO and price in parallel when list_ctx is present to reduce latency.
pub(crate) async fn resolve_direct_price_opt(
    ctx: &FeeCalculationContext<'_>,
    sin: i64,
    sout: i64,
) -> Option<PriceDto> {
    if let Some(ctx_list) = ctx.list_ctx {
        let cycle_id = get_segment_cycle_id(sin, sout);
        let db_pool = ctx.db_pool.clone();
        let cache = ctx.cache.clone();
        let (boo_res, price_opt) = tokio::join!(
            get_boo_from_toll_by_sout(db_pool.clone(), cache.clone(), sout),
            get_price(db_pool.clone(), cache.clone(), sin, sout, &ctx.vehicle_type)
        );
        let boo = boo_res.map(|pb| pb.str).unwrap_or_else(|_| "3".to_string());
        let seg_black = segment_in_black_list(sout, cycle_id, &ctx_list.black_list, &boo);
        if seg_black {
            return price_opt;
        }
        let eff_vehicle =
            effective_vehicle_type(sin, sout, &ctx_list.ex_list, &boo, &ctx.vehicle_type);
        if eff_vehicle == ctx.vehicle_type {
            return price_opt;
        }
        get_segment_price_with_ex(
            ctx.db_pool.clone(),
            ctx.cache.clone(),
            sin,
            sout,
            &ctx.vehicle_type,
            &ctx.ticket_type,
            &ctx_list.ex_list,
            &ctx_list.ex_price_list,
            &boo,
        )
        .await
    } else {
        get_price(
            ctx.db_pool.clone(),
            ctx.cache.clone(),
            sin,
            sout,
            &ctx.vehicle_type,
        )
        .await
    }
}

/// Handle route with single node (same station).
pub(crate) async fn handle_same_station_with_route(
    ctx: &FeeCalculationContext<'_>,
    route: &[i64],
    direct_price_opt: Option<&PriceDto>,
    builder: &RatingDetailBuilder,
) -> Result<(i32, Vec<RatingDetail>), TollFeeError> {
    let t = route[0];
    match direct_price_opt {
        Some(direct_price) => {
            let boo = get_boo_from_toll_by_sout(ctx.db_pool.clone(), ctx.cache.clone(), t)
                .await
                .unwrap_or_else(|_| ParsedBoo::from_price(direct_price));
            let (detail, total) =
                build_same_station_detail_for_direct_price(ctx, t, direct_price, builder, &boo);
            Ok((total, vec![detail]))
        }
        None => {
            let boo_from_toll =
                get_boo_from_toll_by_sout(ctx.db_pool.clone(), ctx.cache.clone(), t).await?;
            let cycle_id = get_segment_cycle_id(t, t);
            let seg_black =
                segment_in_black_list(t, cycle_id, ctx.black_list(), &boo_from_toll.str);
            let seg_white =
                segment_is_whitelisted(t, cycle_id, ctx.white_list(), &boo_from_toll.str);
            let seg_sub = segment_covered_by_subscription(t, t, &ctx.subscription_set);
            if seg_black {
                tracing::warn!(
                    toll = t,
                    "[Processor] TollFee same station no price, segment in black list"
                );
                Err(TollFeeError::SameStationNoPrice)
            } else if seg_white {
                let stage_id = get_stage_id_for_segment(t, t);
                let (detail_toll_a, detail_toll_b) = stage_id
                    .and_then(get_toll_a_toll_b_for_stage_id)
                    .unwrap_or((t, t));
                let detail = create_whitelist_rating_detail(
                    builder,
                    detail_toll_a,
                    detail_toll_b,
                    boo_from_toll.clone(),
                    None,
                    None,
                    stage_id,
                    cycle_id,
                );
                tracing::debug!(toll = t, "[Processor] TollFee same station no price, whitelisted, added one rating detail");
                Ok((0, vec![detail]))
            } else if seg_sub {
                let stage_id = get_stage_id_for_segment(t, t);
                let (detail_toll_a, detail_toll_b) = stage_id
                    .and_then(get_toll_a_toll_b_for_stage_id)
                    .unwrap_or((t, t));
                let detail = create_subscription_rating_detail(
                    builder,
                    detail_toll_a,
                    detail_toll_b,
                    boo_from_toll,
                    None,
                    None,
                    stage_id,
                    ctx.subscription_history,
                    ctx.ex_price_list(),
                    ctx.trans_datetime,
                );
                tracing::debug!(toll = t, "[Processor] TollFee same station no price, subscription, added one rating detail");
                Ok((0, vec![detail]))
            } else {
                tracing::warn!(
                    toll = t,
                    "[Processor] TollFee same station, no price config"
                );
                Err(TollFeeError::SameStationNoPrice)
            }
        }
    }
}

/// Compute fee for one stage_id of a segment (toll_a, toll_b). Produces one RatingDetail. Used when segment has multiple stage_ids.
/// ratingDetail.toll_a_id/toll_b_id are taken from TOLL_STAGE for this stage_id (cycle_id = toll_stage_id) when available.
async fn calculate_fee_for_one_stage_id(
    ctx: &FeeCalculationContext<'_>,
    toll_a: i64,
    toll_b: i64,
    sid: i64,
    builder: &RatingDetailBuilder,
    boo: &ParsedBoo,
    cycle_id: Option<i64>,
) -> Result<(i32, RatingDetail), TollFeeError> {
    let (detail_toll_a, detail_toll_b) =
        get_toll_a_toll_b_for_stage_id(sid).unwrap_or((toll_a, toll_b));

    let boo_str = &boo.str;
    let black_list = ctx.black_list();
    let white_list = ctx.white_list();
    let ex_list = ctx.ex_list();

    let seg_black = segment_in_black_list(toll_b, cycle_id, black_list, boo_str);
    let eff_vehicle = if seg_black {
        ctx.vehicle_type.clone()
    } else {
        effective_vehicle_type(toll_a, toll_b, ex_list, boo_str, &ctx.vehicle_type)
    };

    let seg_white = segment_is_whitelisted(toll_b, cycle_id, white_list, boo_str);
    let seg_sub = segment_covered_by_subscription(toll_a, toll_b, &ctx.subscription_set);

    if seg_white {
        let detail = create_whitelist_rating_detail(
            builder,
            detail_toll_a,
            detail_toll_b,
            boo.clone(),
            None,
            None,
            Some(sid),
            cycle_id,
        );
        return Ok((0, detail));
    }

    if seg_sub {
        let sub_entry = get_subscription_entry_for_stage(
            sid,
            boo_str,
            ctx.subscription_history,
            ctx.trans_datetime,
        );
        let (price_id, bot_id, stage_id_for_detail) = sub_entry
            .map(|e| {
                if e.price_id.is_some() {
                    (e.price_id, e.bot_id, Some(e.stage_id))
                } else {
                    (None, None, Some(sid))
                }
            })
            .unwrap_or((None, None, Some(sid)));
        let detail = create_subscription_rating_detail(
            builder,
            detail_toll_a,
            detail_toll_b,
            boo.clone(),
            price_id,
            bot_id,
            stage_id_for_detail,
            ctx.subscription_history,
            ctx.ex_price_list(),
            ctx.trans_datetime,
        );
        return Ok((0, detail));
    }

    let price_opt =
        get_price_by_stage_id(ctx.db_pool.clone(), ctx.cache.clone(), sid, &eff_vehicle).await;

    match price_opt {
        Some(price) => {
            if !price_is_valid_at_time(
                price.effect_datetime.as_deref(),
                price.end_datetime.as_deref(),
                ctx.trans_datetime,
            ) {
                tracing::warn!(
                    toll_a,
                    toll_b,
                    stage_id = sid,
                    "[Processor] TollFee segment price expired"
                );
                return Err(TollFeeError::PriceExpiredForSegment { toll_a, toll_b });
            }
            let price_boo = ParsedBoo::from_price(&price);
            let seg_black_by_price =
                segment_in_black_list(toll_b, cycle_id, black_list, &price_boo.str);
            let seg_white_by_price =
                segment_is_whitelisted(toll_b, cycle_id, white_list, &price_boo.str);

            if seg_white_by_price {
                let detail = create_whitelist_rating_detail(
                    builder,
                    detail_toll_a,
                    detail_toll_b,
                    boo.clone(),
                    Some(price.price_id),
                    price.bot_id,
                    price.stage_id,
                    cycle_id,
                );
                return Ok((0, detail));
            }

            let seg_sub_by_stage = is_subscribed_by_stage_id(
                price.stage_id,
                &price_boo.str,
                ctx.subscription_history,
                ctx.subscription_stage_ids,
                ctx.trans_datetime,
            );
            if seg_sub_by_stage {
                let sub_entry = price.stage_id.and_then(|stage_id| {
                    get_subscription_entry_for_stage(
                        stage_id,
                        &price_boo.str,
                        ctx.subscription_history,
                        ctx.trans_datetime,
                    )
                });
                let (price_id, bot_id, stage_id) = sub_entry
                    .and_then(|e| {
                        e.price_id
                            .map(|pid| (Some(pid), e.bot_id, Some(e.stage_id)))
                    })
                    .unwrap_or((Some(price.price_id), price.bot_id, price.stage_id));
                let detail = create_subscription_rating_detail(
                    builder,
                    detail_toll_a,
                    detail_toll_b,
                    price_boo,
                    price_id,
                    bot_id,
                    stage_id,
                    ctx.subscription_history,
                    ctx.ex_price_list(),
                    ctx.trans_datetime,
                );
                return Ok((0, detail));
            }

            let price_amount = calculate_amount_from_price(&price, ctx.trans_datetime);
            let final_price_ticket_type = if seg_black_by_price {
                price_ticket_type_for_bl(cycle_id)
            } else {
                crate::price_ticket_type::from_db_string(price.price_ticket_type.as_deref())
            };
            let detail = create_regular_rating_detail(
                builder,
                detail_toll_a,
                detail_toll_b,
                price_boo,
                price_amount,
                final_price_ticket_type,
                Some(price.price_id),
                price.bot_id,
                price.stage_id,
                false,
            );
            Ok((price_amount, detail))
        }
        None => {
            tracing::warn!(
                toll_a,
                toll_b,
                stage_id = sid,
                "[Processor] TollFee no price for segment (by stage_id)"
            );
            Err(TollFeeError::NoPriceConfigForSegment { toll_a, toll_b })
        }
    }
}

/// Compute fee for one segment (toll_a, toll_b) given stage_ids from CLOSED_CYCLE_TRANSITION_STAGE.
/// Produces one RatingDetail per stage_id (e.g. 2 stage_ids → 2 RatingDetails). Total amount = sum of all.
pub(crate) async fn calculate_fee_for_segment_by_stage_id(
    ctx: &FeeCalculationContext<'_>,
    toll_a: i64,
    toll_b: i64,
    stage_ids: &[i64],
    builder: &RatingDetailBuilder,
) -> Result<(i32, Vec<RatingDetail>), TollFeeError> {
    if stage_ids.is_empty() {
        tracing::warn!(
            toll_a,
            toll_b,
            "[Processor] TollFee segment has no stage_id"
        );
        return Err(TollFeeError::NoPriceConfigForSegment { toll_a, toll_b });
    }

    let boo = get_boo_from_toll_by_sout(ctx.db_pool.clone(), ctx.cache.clone(), toll_b).await?;
    let cycle_id = get_segment_cycle_id(toll_a, toll_b);

    let mut total_amount = 0i32;
    let mut details = Vec::with_capacity(stage_ids.len());
    for &sid in stage_ids {
        let (amount, detail) =
            calculate_fee_for_one_stage_id(ctx, toll_a, toll_b, sid, builder, &boo, cycle_id)
                .await?;
        total_amount += amount;
        details.push(detail);
    }
    Ok((total_amount, details))
}

/// Run fee calculation for multiple segments in parallel. Reduces total time when there are several segments.
/// Each segment is (toll_a, toll_b, stage_ids). Returns combined (total_amount, all details) or first error.
pub(crate) async fn calculate_fees_for_segments_parallel(
    ctx: &FeeCalculationContext<'_>,
    segments: &[(i64, i64, Vec<i64>)],
    builder: &RatingDetailBuilder,
) -> Result<(i32, Vec<RatingDetail>), TollFeeError> {
    if segments.is_empty() {
        return Ok((0, vec![]));
    }
    // Single segment: avoid allocation and join_all overhead.
    if segments.len() == 1 {
        let (toll_a, toll_b, stage_ids) = &segments[0];
        return calculate_fee_for_segment_by_stage_id(
            ctx,
            *toll_a,
            *toll_b,
            stage_ids.as_slice(),
            builder,
        )
        .await;
    }
    let futures = segments.iter().map(|(toll_a, toll_b, stage_ids)| {
        calculate_fee_for_segment_by_stage_id(ctx, *toll_a, *toll_b, stage_ids.as_slice(), builder)
    });
    let results = join_all(futures).await;
    let mut total_amount = 0i32;
    let mut all_details = Vec::with_capacity(results.len());
    for res in results {
        let (amount, details) = res?;
        total_amount += amount;
        all_details.extend(details);
    }
    Ok((total_amount, all_details))
}

/// Handle case when route is not found.
pub(crate) async fn handle_no_route(
    ctx: &FeeCalculationContext<'_>,
    sin: i64,
    sout: i64,
    direct_price_opt: Option<&PriceDto>,
    builder: &RatingDetailBuilder,
) -> Result<(i32, Vec<RatingDetail>), TollFeeError> {
    let white_list = ctx.white_list();
    let black_list = ctx.black_list();

    match direct_price_opt {
        Some(direct_price) => {
            // BOO for segment (sin, sout) = exit station sout.
            let boo = get_boo_from_toll_by_sout(ctx.db_pool.clone(), ctx.cache.clone(), sout)
                .await
                .unwrap_or_else(|_| ParsedBoo::from_price(direct_price));
            let cycle_id = get_segment_cycle_id(sin, sout);
            let seg_black = segment_in_black_list(sout, cycle_id, black_list, &boo.str);
            let seg_white = segment_is_whitelisted(sout, cycle_id, white_list, &boo.str);

            let (detail_toll_a, detail_toll_b) = direct_price
                .stage_id
                .and_then(get_toll_a_toll_b_for_stage_id)
                .unwrap_or((sin, sout));

            if seg_black {
                if !price_is_valid_at_time(
                    direct_price.effect_datetime.as_deref(),
                    direct_price.end_datetime.as_deref(),
                    ctx.trans_datetime,
                ) {
                    tracing::warn!(
                        sin,
                        sout,
                        "[Processor] TollFee no route black segment price expired"
                    );
                    return Err(TollFeeError::PriceExpiredForSegment {
                        toll_a: sin,
                        toll_b: sout,
                    });
                }
                let price_amount = calculate_amount_from_price(direct_price, ctx.trans_datetime);
                let detail = create_regular_rating_detail(
                    builder,
                    detail_toll_a,
                    detail_toll_b,
                    ParsedBoo::from_price(direct_price),
                    price_amount,
                    price_ticket_type_for_bl(cycle_id),
                    Some(direct_price.price_id),
                    direct_price.bot_id,
                    direct_price.stage_id,
                    false,
                );
                tracing::debug!(
                    sin,
                    sout,
                    amount = price_amount,
                    "[Processor] TollFee no route direct price black"
                );
                return Ok((price_amount, vec![detail]));
            }

            if seg_white {
                let detail = create_whitelist_rating_detail(
                    builder,
                    detail_toll_a,
                    detail_toll_b,
                    boo.clone(),
                    Some(direct_price.price_id),
                    direct_price.bot_id,
                    direct_price.stage_id,
                    cycle_id,
                );
                tracing::debug!(
                    sin,
                    sout,
                    "[Processor] TollFee no route direct price whitelist"
                );
                return Ok((0, vec![detail]));
            }

            if !price_is_valid_at_time(
                direct_price.effect_datetime.as_deref(),
                direct_price.end_datetime.as_deref(),
                ctx.trans_datetime,
            ) {
                tracing::warn!(sin, sout, effect_datetime = ?direct_price.effect_datetime, end_datetime = ?direct_price.end_datetime, "[Processor] TollFee no route, price expired");
                return Err(TollFeeError::PriceExpiredForSegment {
                    toll_a: sin,
                    toll_b: sout,
                });
            }

            let no_route_boo = ParsedBoo::from_price(direct_price);
            if let Some(sid) = direct_price.stage_id {
                let is_subscribed = is_subscribed_by_stage_id(
                    Some(sid),
                    &no_route_boo.str,
                    ctx.subscription_history,
                    ctx.subscription_stage_ids,
                    ctx.trans_datetime,
                );
                if is_subscribed {
                    let detail = create_subscription_rating_detail(
                        builder,
                        detail_toll_a,
                        detail_toll_b,
                        ParsedBoo::from_price(direct_price),
                        Some(direct_price.price_id),
                        direct_price.bot_id,
                        direct_price.stage_id,
                        ctx.subscription_history,
                        ctx.ex_price_list(),
                        ctx.trans_datetime,
                    );
                    tracing::debug!(
                        sin,
                        sout,
                        "[Processor] TollFee no route direct price subscription by stage"
                    );
                    return Ok((0, vec![detail]));
                }
            }

            if segment_covered_by_subscription(sin, sout, &ctx.subscription_set) {
                let detail = create_subscription_rating_detail(
                    builder,
                    detail_toll_a,
                    detail_toll_b,
                    no_route_boo.clone(),
                    Some(direct_price.price_id),
                    direct_price.bot_id,
                    direct_price.stage_id,
                    ctx.subscription_history,
                    ctx.ex_price_list(),
                    ctx.trans_datetime,
                );
                tracing::debug!(
                    sin,
                    sout,
                    "[Processor] TollFee no route direct price subscription by segment"
                );
                return Ok((0, vec![detail]));
            }

            let price_amount = calculate_amount_from_price(direct_price, ctx.trans_datetime);
            let seg_black_by_price = segment_in_black_list(
                sout,
                get_segment_cycle_id(sin, sout),
                black_list,
                &no_route_boo.str,
            );
            let final_price_ticket_type = if seg_black_by_price {
                price_ticket_type_for_bl(get_segment_cycle_id(sin, sout))
            } else {
                crate::price_ticket_type::from_db_string(direct_price.price_ticket_type.as_deref())
            };
            let detail = create_regular_rating_detail(
                builder,
                detail_toll_a,
                detail_toll_b,
                ParsedBoo::from_price(direct_price),
                price_amount,
                final_price_ticket_type,
                Some(direct_price.price_id),
                direct_price.bot_id,
                direct_price.stage_id,
                false,
            );
            tracing::debug!(
                sin,
                sout,
                amount = price_amount,
                "[Processor] TollFee no route direct price regular"
            );
            Ok((price_amount, vec![detail]))
        }
        None => {
            let boo_from_toll =
                get_boo_from_toll_by_sout(ctx.db_pool.clone(), ctx.cache.clone(), sout).await?;
            let cycle_id = get_segment_cycle_id(sin, sout);
            let seg_black = segment_in_black_list(sout, cycle_id, black_list, &boo_from_toll.str);
            let seg_white = segment_is_whitelisted(sout, cycle_id, white_list, &boo_from_toll.str);
            let seg_sub = segment_covered_by_subscription(sin, sout, &ctx.subscription_set);
            if seg_black {
                tracing::warn!(
                    sin,
                    sout,
                    "[Processor] TollFee no route, segment in black list but no direct price"
                );
                return Err(TollFeeError::NoRouteNoPrice);
            }
            if seg_white {
                let stage_id = get_stage_id_for_segment(sin, sout);
                let (detail_toll_a, detail_toll_b) = stage_id
                    .and_then(get_toll_a_toll_b_for_stage_id)
                    .unwrap_or((sin, sout));
                let detail = create_whitelist_rating_detail(
                    builder,
                    detail_toll_a,
                    detail_toll_b,
                    boo_from_toll.clone(),
                    None,
                    None,
                    stage_id,
                    cycle_id,
                );
                tracing::debug!(
                    sin,
                    sout,
                    "[Processor] TollFee no route no price, whitelisted, added one rating detail"
                );
                Ok((0, vec![detail]))
            } else if seg_sub {
                let stage_id = get_stage_id_for_segment(sin, sout);
                let (detail_toll_a, detail_toll_b) = stage_id
                    .and_then(get_toll_a_toll_b_for_stage_id)
                    .unwrap_or((sin, sout));
                let detail = create_subscription_rating_detail(
                    builder,
                    detail_toll_a,
                    detail_toll_b,
                    boo_from_toll,
                    None,
                    None,
                    stage_id,
                    ctx.subscription_history,
                    ctx.ex_price_list(),
                    ctx.trans_datetime,
                );
                tracing::debug!(
                    sin,
                    sout,
                    "[Processor] TollFee no route no price, subscription, added one rating detail"
                );
                Ok((0, vec![detail]))
            } else {
                tracing::warn!(
                    sin,
                    sout,
                    "[Processor] TollFee no route, no direct price, not whitelist/subscription"
                );
                Err(TollFeeError::NoRouteNoPrice)
            }
        }
    }
}
