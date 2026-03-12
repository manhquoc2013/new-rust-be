//! Toll fee calculation errors: missing price config, route/BOO not found.

use std::fmt;

/// Error when price config is missing or fee cannot be computed.
#[derive(Debug)]
pub enum TollFeeError {
    /// Segment (toll_a, toll_b) has no price config.
    NoPriceConfigForSegment { toll_a: i64, toll_b: i64 },
    /// Route not found and no price config for station pair.
    NoRouteNoPrice,
    /// Same station (entry = exit) has no price config.
    SameStationNoPrice,
    /// Segment price has expired, fee cannot be computed.
    PriceExpiredForSegment { toll_a: i64, toll_b: i64 },
    /// BOO for station (sout) not found in TOLL table.
    BooNotFoundForToll { toll_id: i64 },
    /// Price config or route not found for segment (toll_a, toll_b).
    RouteOrPriceConfigNotFound { toll_a: i64, toll_b: i64 },
}

impl fmt::Display for TollFeeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TollFeeError::NoPriceConfigForSegment { toll_a, toll_b } => {
                write!(f, "No price config for segment ({}, {})", toll_a, toll_b)
            }
            TollFeeError::NoRouteNoPrice => {
                write!(f, "Route not found and no price config")
            }
            TollFeeError::SameStationNoPrice => {
                write!(f, "Same station has no price config")
            }
            TollFeeError::PriceExpiredForSegment { toll_a, toll_b } => {
                write!(f, "Segment ({}, {}) price has expired", toll_a, toll_b)
            }
            TollFeeError::BooNotFoundForToll { toll_id } => {
                write!(f, "BOO for station {} not found in TOLL table", toll_id)
            }
            TollFeeError::RouteOrPriceConfigNotFound { toll_a, toll_b } => {
                write!(
                    f,
                    "Price config or route not found for segment ({}, {})",
                    toll_a, toll_b
                )
            }
        }
    }
}

impl std::error::Error for TollFeeError {}
