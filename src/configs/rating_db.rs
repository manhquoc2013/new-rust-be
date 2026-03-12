//! DB connection pool RATING (lazy static).

use std::sync::Arc;

use super::pool_factory::{create_pool, OdbcConnectionManager};
use once_cell::sync::Lazy;
use r2d2::Pool;

pub static RATING_DB: Lazy<Arc<Pool<OdbcConnectionManager>>> =
    Lazy::new(|| Arc::new(create_pool("RATING")));
