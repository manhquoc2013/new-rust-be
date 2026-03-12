//! Types used in fee calculation: context, BOO, segment, builder.

use std::collections::HashSet;
use std::sync::Arc;

use r2d2::Pool;

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::data::dto::price_dto::PriceDto;
use crate::cache::data::dto::subscription_history_dto::SubscriptionHistoryDto;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::models::VDTCmessages::RatingDetail;

use super::helpers::segment_key;

/// List context for white/black/ex by etag (similar to Java: whiteList, blackList, exListData, exPriceListData). WB/Ex apply by price BOO: only use records with BOO matching price.boo (or BOO null/empty = all BOO). WB: only when segment cycle_id (from TOLL_STAGE for tollA,tollB) equals WB entry cycle_id (if entry has cycle_id).
#[derive(Default, Clone, Debug)]
pub struct TollFeeListContext {
    /// (toll_id, cycle_id, boo): white list; cycle_id None = no cycle filter; toll_id 0 = nationwide, apply only for matching tollB when non-zero.
    pub white_list: Vec<(i64, Option<i64>, Option<String>)>,
    /// (toll_id, cycle_id, boo): black list, same structure.
    pub black_list: Vec<(i64, Option<i64>, Option<String>)>,
    /// (station_id, boo, vehicle_type, type_station): ex list by BOO; type_station 0/1/2.
    pub ex_list: Vec<(i64, Option<String>, String, Option<String>)>,
    /// (station_id, boo, price_ticket_type, type_station): ex price list by BOO; type_station 0/1/2.
    pub ex_price_list: Vec<(i64, Option<String>, i32, Option<String>)>,
}

/// Parsed BOO (string + int).
#[derive(Clone, Debug)]
pub(crate) struct ParsedBoo {
    pub str: String,
    pub int: i32,
}

impl ParsedBoo {
    pub(crate) fn from_str(s: &str) -> Self {
        let str = s.trim().to_string();
        let int = str.parse::<i32>().unwrap_or(3);
        Self { str, int }
    }

    pub(crate) fn from_price(price: &PriceDto) -> Self {
        Self::from_str(price.boo.as_deref().unwrap_or("3"))
    }

    pub(crate) fn default() -> Self {
        Self::from_str("3")
    }
}

/// Builder to create RatingDetail from fields.
/// The `toll_a` and `toll_b` passed to `build` are the segment (TOLL_A, TOLL_B) from TOLL_STAGE for the stage used in pricing.
pub(crate) struct RatingDetailBuilder {
    pub subscription_id: String,
    pub vehicle_type: i32,
    pub ticket_type: String,
}

impl RatingDetailBuilder {
    pub(crate) fn new(subscription_id: String, vehicle_type: i32, ticket_type: String) -> Self {
        Self {
            subscription_id,
            vehicle_type,
            ticket_type,
        }
    }

    /// Builds a RatingDetail. `toll_a` and `toll_b` are the segment (TOLL_A, TOLL_B) from TOLL_STAGE for this stage.
    pub(crate) fn build(
        &self,
        toll_a: i64,
        toll_b: i64,
        boo: i32,
        price_amount: i32,
        price_ticket_type: i32,
        price_id: Option<i64>,
        bot_id: Option<i64>,
        stage_id: Option<i64>,
        ticket_type_override: Option<String>,
    ) -> RatingDetail {
        RatingDetail {
            boo,
            toll_a_id: toll_a as i32,
            toll_b_id: toll_b as i32,
            ticket_type: ticket_type_override.unwrap_or_else(|| self.ticket_type.clone()),
            subscription_id: self.subscription_id.clone(),
            price_ticket_type,
            price_amount,
            vehicle_type: self.vehicle_type,
            price_id,
            bot_id,
            stage_id,
        }
    }
}

/// Context for computing fee for a segment.
pub(crate) struct FeeCalculationContext<'a> {
    pub db_pool: Arc<Pool<OdbcConnectionManager>>,
    pub cache: Arc<CacheManager>,
    pub vehicle_type: String,
    pub ticket_type: String,
    pub subscription_id: String,
    pub list_ctx: Option<&'a TollFeeListContext>,
    pub subscription_set: HashSet<(i64, i64)>,
    pub subscription_history: Option<&'a [SubscriptionHistoryDto]>,
    pub subscription_stage_ids: Option<&'a HashSet<i64>>,
    pub trans_datetime: Option<chrono::NaiveDateTime>,
}

impl<'a> FeeCalculationContext<'a> {
    pub(crate) fn new(
        db_pool: Arc<Pool<OdbcConnectionManager>>,
        cache: Arc<CacheManager>,
        vehicle_type: i32,
        ticket_type: &str,
        subscriber_id: Option<i64>,
        list_ctx: Option<&'a TollFeeListContext>,
        subscription_stages: Option<&[(i64, i64)]>,
        subscription_stage_ids: Option<&'a HashSet<i64>>,
        subscription_history: Option<&'a [SubscriptionHistoryDto]>,
        trans_datetime: Option<chrono::NaiveDateTime>,
    ) -> Self {
        let subscription_set: HashSet<(i64, i64)> = subscription_stages
            .map(|s| s.iter().map(|&(a, b)| segment_key(a, b)).collect())
            .unwrap_or_default();
        let subscription_id = subscriber_id.map(|id| id.to_string()).unwrap_or_default();
        Self {
            db_pool,
            cache,
            vehicle_type: vehicle_type.to_string(),
            ticket_type: ticket_type.to_string(),
            subscription_id,
            list_ctx,
            subscription_set,
            subscription_history,
            subscription_stage_ids,
            trans_datetime,
        }
    }

    pub(crate) fn white_list(&self) -> &[(i64, Option<i64>, Option<String>)] {
        self.list_ctx
            .map(|c| c.white_list.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn black_list(&self) -> &[(i64, Option<i64>, Option<String>)] {
        self.list_ctx
            .map(|c| c.black_list.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn ex_list(&self) -> &[(i64, Option<String>, String, Option<String>)] {
        self.list_ctx.map(|c| c.ex_list.as_slice()).unwrap_or(&[])
    }

    pub(crate) fn ex_price_list(&self) -> &[(i64, Option<String>, i32, Option<String>)] {
        self.list_ctx
            .map(|c| c.ex_price_list.as_slice())
            .unwrap_or(&[])
    }
}
