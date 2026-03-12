//! ETDR (Entry Transaction Data Record): types, constructors, updates.

#![allow(dead_code)]

mod constructors;
mod types;
mod updates;

pub use types::*;
// constructors và updates bổ sung impl ETDR, không cần re-export riêng
