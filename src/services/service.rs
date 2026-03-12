//! Trait Service cache-aside, ServiceError, generate_cache_key, DEFAULT_CACHE_TTL.

use crate::constants::cache;
use crate::db::error::DbError;
use crate::services::cache::CacheError;
use std::fmt::Display;
use std::time::Duration;

/// Trait service cho business logic với pattern cache-aside.
#[async_trait::async_trait]
pub trait Service<T, ID>
where
    T: Clone + Send + Sync + 'static,
    ID: Display + Clone + Send + Sync + 'static,
{
    /// Lấy entity theo ID (cache-aside: memory → distributed → DB, rồi ghi cache).
    async fn get_by_id(&self, id: ID) -> Result<Option<T>, ServiceError>;

    /// Lưu entity (write-through: DB trước, thất bại thì ghi cache retry, thành công thì cập nhật cache).
    async fn save(&self, entity: &T) -> Result<ID, ServiceError>;

    /// Cập nhật entity với pattern write-through.
    async fn update(&self, id: ID, entity: &T) -> Result<bool, ServiceError>;

    /// Xóa entity và invalidate caches.
    async fn delete(&self, id: ID) -> Result<bool, ServiceError>;
}

/// Các loại lỗi thao tác service.
#[derive(Debug)]
pub enum ServiceError {
    DatabaseError(DbError),
    CacheError(CacheError),
    NotFound(String),
    ValidationError(String),
    Other(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::DatabaseError(e) => write!(f, "Database error: {}", e),
            ServiceError::CacheError(e) => write!(f, "Cache error: {}", e),
            ServiceError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ServiceError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            ServiceError::Other(msg) => write!(f, "Service error: {}", msg),
        }
    }
}

impl std::error::Error for ServiceError {}

impl From<DbError> for ServiceError {
    fn from(err: DbError) -> Self {
        ServiceError::DatabaseError(err)
    }
}

impl From<CacheError> for ServiceError {
    fn from(err: CacheError) -> Self {
        ServiceError::CacheError(err)
    }
}

/// Tạo cache key từ prefix và id.
pub fn generate_cache_key<T: Display>(prefix: &str, id: T) -> String {
    format!("{}:{}", prefix, id)
}

/// TTL mặc định cho cache (1 giờ), từ constants::cache::DEFAULT_CACHE_TTL_SECS.
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(cache::DEFAULT_CACHE_TTL_SECS);
