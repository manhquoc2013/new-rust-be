//! DB connection pool CRM (lazy static).

use super::pool_factory::{create_pool, OdbcConnectionManager};
use once_cell::sync::Lazy;
use r2d2::Pool;

pub static CRM_DB: Lazy<Pool<OdbcConnectionManager>> = Lazy::new(|| create_pool("CRM"));
