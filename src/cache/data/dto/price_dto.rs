//! Price table DTO (price): price_id, toll_a/toll_b, boo, price_ticket_type, amount.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceDto {
    pub price_id: i64,
    pub toll_a: Option<i64>,
    pub toll_b: Option<i64>,
    pub boo: Option<String>,
    pub price_amount: Option<i64>,
    pub vehicle_type: Option<String>,
    pub price_ticket_type: Option<String>,
    pub price_type: Option<String>,
    pub turning_code: Option<String>,
    pub effect_datetime: Option<String>,
    pub end_datetime: Option<String>,
    /// BOT_ID từ RATING_OWNER.BOT_TOLL
    pub bot_id: Option<i64>,
    /// STAGE_ID từ RATING_OWNER.PRICE (P.STAGE_ID)
    pub stage_id: Option<i64>,
}
