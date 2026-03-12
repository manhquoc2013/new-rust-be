//! Fee by direct price: same station.

use crate::cache::data::closed_cycle_transition_stage_cache::{
    get_segment_cycle_id, get_toll_a_toll_b_for_stage_id,
};
use crate::cache::data::dto::price_dto::PriceDto;
use crate::models::RatingDetail;

use super::detail::{
    create_regular_rating_detail, create_subscription_rating_detail, create_whitelist_rating_detail,
};
use super::helpers::{
    price_ticket_type_for_bl, segment_covered_by_subscription, segment_in_black_list,
    segment_is_whitelisted,
};
use super::segment::{calculate_amount_from_price, get_boo_from_toll_by_sout};
use super::types::{FeeCalculationContext, ParsedBoo, RatingDetailBuilder};

/// Build one rating detail for same-station (t,t) with direct price. Returns (detail, total_amount). Checks by segment BOO (toll_b = t): black list highest priority, then white list. WB uses BOO of toll_b.
pub(crate) fn build_same_station_detail_for_direct_price(
    ctx: &FeeCalculationContext<'_>,
    t: i64,
    direct_price: &PriceDto,
    builder: &RatingDetailBuilder,
    boo: &ParsedBoo,
) -> (RatingDetail, i32) {
    let (detail_toll_a, detail_toll_b) = direct_price
        .stage_id
        .and_then(get_toll_a_toll_b_for_stage_id)
        .unwrap_or((t, t));
    let direct_boo = ParsedBoo::from_price(direct_price);
    let price_ticket_type =
        crate::price_ticket_type::from_db_string(direct_price.price_ticket_type.as_deref());
    let white_list = ctx.white_list();
    let black_list = ctx.black_list();
    let cycle_id = get_segment_cycle_id(t, t);
    let seg_black = segment_in_black_list(t, cycle_id, black_list, &boo.str);
    let seg_white = segment_is_whitelisted(t, cycle_id, white_list, &boo.str);
    let seg_sub = segment_covered_by_subscription(t, t, &ctx.subscription_set);
    let detail = if seg_black {
        let amount = calculate_amount_from_price(direct_price, ctx.trans_datetime);
        let detail = create_regular_rating_detail(
            builder,
            detail_toll_a,
            detail_toll_b,
            direct_boo,
            amount,
            price_ticket_type_for_bl(cycle_id),
            Some(direct_price.price_id),
            direct_price.bot_id,
            direct_price.stage_id,
            false,
        );
        return (detail, amount);
    } else if seg_white {
        create_whitelist_rating_detail(
            builder,
            detail_toll_a,
            detail_toll_b,
            boo.clone(),
            Some(direct_price.price_id),
            direct_price.bot_id,
            direct_price.stage_id,
            cycle_id,
        )
    } else if seg_sub {
        create_subscription_rating_detail(
            builder,
            detail_toll_a,
            detail_toll_b,
            direct_boo.clone(),
            Some(direct_price.price_id),
            direct_price.bot_id,
            direct_price.stage_id,
            ctx.subscription_history,
            ctx.ex_price_list(),
            ctx.trans_datetime,
        )
    } else {
        let amount = calculate_amount_from_price(direct_price, ctx.trans_datetime);
        let detail = create_regular_rating_detail(
            builder,
            detail_toll_a,
            detail_toll_b,
            direct_boo,
            amount,
            price_ticket_type,
            Some(direct_price.price_id),
            direct_price.bot_id,
            direct_price.stage_id,
            false,
        );
        return (detail, amount);
    };
    (detail, 0)
}
