//! Toll fee service: entry point `calculate_toll_fee`.
//!
//! Segments from CLOSED_CYCLE_TRANSITION_STAGE: (TOLL_A, TOLL_B) -> Vec<CLOSED_CYCLE_TRANSITION_ID>. Check WB/ExList/Blacklist then get price by stage_id, no merge.
//!
//! **RatingDetail toll_a / toll_b**: In fee calculation, each rating detail's `toll_a_id` and `toll_b_id` are the segment (TOLL_A, TOLL_B) from TOLL_STAGE for the stage used in pricing — i.e. the same (toll_a, toll_b) that identify the segment in CLOSED_CYCLE_TRANSITION_STAGE / TOLL_STAGE.

use std::collections::HashSet;
use std::sync::Arc;

use r2d2::Pool;

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::data::closed_cycle_transition_stage_cache::get_segments_for_leg;
use crate::cache::data::dto::subscription_history_dto::SubscriptionHistoryDto;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::models::RatingDetail;

use super::error::TollFeeError;
use super::flow::{
    calculate_fee_for_segment_by_stage_id, calculate_fees_for_segments_parallel,
    handle_same_station_with_route, resolve_direct_price_opt,
};
use super::types::{FeeCalculationContext, RatingDetailBuilder, TollFeeListContext};

/// Toll fee service by entry/exit station and vehicle type.
pub struct TollFeeService;

impl TollFeeService {
    /// Computes toll fee and builds RatingDetails.
    pub async fn calculate_toll_fee(
        db_pool: Arc<Pool<OdbcConnectionManager>>,
        cache: Arc<CacheManager>,
        station_in: i32,
        station_out: i32,
        vehicle_type: i32,
        ticket_type: &str,
        subscriber_id: Option<i64>,
        list_ctx: Option<&TollFeeListContext>,
        subscription_stages: Option<&[(i64, i64)]>,
        subscription_stage_ids: Option<&HashSet<i64>>,
        subscription_history: Option<&[SubscriptionHistoryDto]>,
        trans_datetime: Option<chrono::NaiveDateTime>,
    ) -> Result<(i32, Vec<RatingDetail>), TollFeeError> {
        tracing::debug!(station_in, station_out, vehicle_type, ticket_type = %ticket_type, "[Processor] TollFee calc");

        if list_ctx.is_none() {
            tracing::debug!(
                station_in,
                station_out,
                "[Processor] TollFee WB/ExList not applied (list_ctx is None, etag may be empty)"
            );
        }

        let ctx = FeeCalculationContext::new(
            db_pool.clone(),
            cache.clone(),
            vehicle_type,
            ticket_type,
            subscriber_id,
            list_ctx,
            subscription_stages,
            subscription_stage_ids,
            subscription_history,
            trans_datetime,
        );
        let builder = RatingDetailBuilder::new(
            ctx.subscription_id.clone(),
            vehicle_type,
            ctx.ticket_type.clone(),
        );

        let sin = station_in as i64;
        let sout = station_out as i64;

        // Same station: handle immediately without route lookup.
        if sin == sout {
            let direct_price_opt = resolve_direct_price_opt(&ctx, sin, sout).await;
            let route = vec![sin];
            return handle_same_station_with_route(
                &ctx,
                &route,
                direct_price_opt.as_ref(),
                &builder,
            )
            .await;
        }

        // Segments from CLOSED_CYCLE_TRANSITION_STAGE cache (cache-aside: memory -> cache -> DB).
        let segment_stage_ids =
            get_segments_for_leg(ctx.db_pool.clone(), ctx.cache.clone(), sin, sout).await;

        match segment_stage_ids {
            Some(ref stage_ids) if !stage_ids.is_empty() => {
                tracing::info!(
                    station_in,
                    station_out,
                    "[Processor] TollFee segment from CLOSED_CYCLE_TRANSITION_STAGE, price by stage_id"
                );
                // Single segment: call directly to avoid Vec allocation and stage_ids clone.
                calculate_fee_for_segment_by_stage_id(&ctx, sin, sout, stage_ids, &builder).await
            }
            _ => {
                tracing::warn!(
                    station_in,
                    station_out,
                    "[Processor] TollFee no segment in CLOSED_CYCLE_TRANSITION_STAGE, return error"
                );
                Err(TollFeeError::RouteOrPriceConfigNotFound {
                    toll_a: sin,
                    toll_b: sout,
                })
            }
        }
    }
}
