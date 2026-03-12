//! Shared rating detail type used by BOO protocol (CHECKOUT_RESERVE_BOO) and toll_fee/commit logic.
//! Not part of BOO wire format only — used when building fee lines and mapping to BOORatingDetail.

use std::fmt;

/// Rating detail for one fee line. Used in BOO CHECKOUT_RESERVE_BOO and by toll_fee service.
/// Covers BOOA, BOOB, BOO3.
#[allow(dead_code)]
#[derive(Default, Clone)]
pub struct RatingDetail {
    pub price_id: Option<i64>,
    pub boo: i32,
    pub toll_a_id: i32,
    pub toll_b_id: i32,
    pub ticket_type: String,
    pub subscription_id: String,
    pub price_ticket_type: i32,
    pub price_amount: i32,
    pub vehicle_type: i32,
    /// BOT_ID from RATING_OWNER.BOT_TOLL
    pub bot_id: Option<i64>,
    /// STAGE_ID from RATING_OWNER.PRICE (P.STAGE_ID)
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
