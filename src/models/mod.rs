//! Message and event models: TCOC, BOO, ETDR, TollCache.
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub mod ETDR;
pub mod TCOCmessages;
pub mod TollCache;
pub mod bect_messages;
pub mod rating_detail;

pub use rating_detail::RatingDetail;
