//! Build RatingDetail per case: whitelist, subscription, regular.
//! Each detail's toll_a/toll_b are the segment (TOLL_A, TOLL_B) from TOLL_STAGE for the stage used in pricing.

use crate::cache::data::dto::subscription_history_dto::SubscriptionHistoryDto;
use crate::models::VDTCmessages::RatingDetail;

use super::helpers::{
    effective_ticket_type, get_subscription_price_type, price_ticket_type_for_wb,
};
use super::types::{ParsedBoo, RatingDetailBuilder};

/// Build RatingDetail for subscription case.
pub(crate) fn create_subscription_rating_detail(
    builder: &RatingDetailBuilder,
    toll_a: i64,
    toll_b: i64,
    boo: ParsedBoo,
    price_id: Option<i64>,
    bot_id: Option<i64>,
    stage_id: Option<i64>,
    subscription_history: Option<&[SubscriptionHistoryDto]>,
    ex_price_list: &[(i64, Option<String>, i32, Option<String>)],
    trans_datetime: Option<chrono::NaiveDateTime>,
) -> RatingDetail {
    let sub_price_type = get_subscription_price_type(
        toll_a,
        toll_b,
        stage_id,
        &boo.str,
        subscription_history,
        trans_datetime,
    );
    let sub_price_ticket_type_str =
        effective_ticket_type(toll_a, toll_b, ex_price_list, &boo.str, "0");
    let sub_price_ticket_type = crate::price_ticket_type::from_str(&sub_price_ticket_type_str);
    builder.build(
        toll_a,
        toll_b,
        boo.int,
        0,
        sub_price_ticket_type,
        price_id,
        bot_id,
        stage_id,
        Some(sub_price_type),
    )
}

/// Build RatingDetail for white list case (WB): amount = 0; price_ticket_type = WL_ALL when cycle_id is None or 0, else WL_TOLL.
pub(crate) fn create_whitelist_rating_detail(
    builder: &RatingDetailBuilder,
    toll_a: i64,
    toll_b: i64,
    boo: ParsedBoo,
    price_id: Option<i64>,
    bot_id: Option<i64>,
    stage_id: Option<i64>,
    cycle_id: Option<i64>,
) -> RatingDetail {
    let price_ticket_type = price_ticket_type_for_wb(cycle_id);
    builder.build(
        toll_a,
        toll_b,
        boo.int,
        0,
        price_ticket_type,
        price_id,
        bot_id,
        stage_id,
        None,
    )
}

/// Build RatingDetail for regular case.
pub(crate) fn create_regular_rating_detail(
    builder: &RatingDetailBuilder,
    toll_a: i64,
    toll_b: i64,
    boo: ParsedBoo,
    price_amount: i32,
    price_ticket_type: i32,
    price_id: Option<i64>,
    bot_id: Option<i64>,
    stage_id: Option<i64>,
    _unused: bool,
) -> RatingDetail {
    builder.build(
        toll_a,
        toll_b,
        boo.int,
        price_amount,
        price_ticket_type,
        price_id,
        bot_id,
        stage_id,
        None,
    )
}
