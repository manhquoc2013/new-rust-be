//! Toll fee: rating from cache/DB, subscription, price_ticket_type, route graph.
//!
//! Module layout:
//! - **error**: Fee error codes (TollFeeError)
//! - **types**: TollFeeListContext, ParsedBoo, FeeCalculationContext, RatingDetailBuilder
//! - **helpers**: BOO matching, subscription/price validity, white/black/subscription by segment
//! - **detail**: Build RatingDetail (whitelist, subscription, regular)
//! - **segment**: get price by stage_id / ex (no-route), BOO from TOLL
//! - **direct_price**: Same station with direct price
//! - **flow**: Resolve direct price, same station, segment by stage_id, no route
//! - **service**: TollFeeService and calculate_toll_fee

mod detail;
mod direct_price;
mod error;
mod flow;
pub(crate) mod helpers;
mod segment;
mod service;
mod types;

pub use error::TollFeeError;
pub(crate) use helpers::vehicle_type_from_ex_list_open;
pub use service::TollFeeService;
pub use types::TollFeeListContext;
