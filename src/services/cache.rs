//! Trait MemoryCache/DistributedCache, CacheError, SimpleMemoryCache, PlaceholderDistributedCache.
//! SimpleMemoryCache uses DashMap for concurrent get/set with less contention when cache is large.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use dashmap::DashMap;

/// Các loại lỗi thao tác cache.
#[derive(Debug, Clone)]
pub enum CacheError {
    NotFound(String),
    SerializationError(String),
    DeserializationError(String),
    ConnectionError(String),
    Other(String),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::NotFound(msg) => write!(f, "Cache not found: {}", msg),
            CacheError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            CacheError::DeserializationError(msg) => write!(f, "Deserialization error: {}", msg),
            CacheError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            CacheError::Other(msg) => write!(f, "Cache error: {}", msg),
        }
    }
}

impl std::error::Error for CacheError {}

/// Trait cache trong bộ nhớ (ví dụ: HashMap, LRU).
pub trait MemoryCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn get(&self, key: &str) -> Result<Option<T>, CacheError>;
    fn set(&self, key: &str, value: T, ttl: Option<Duration>) -> Result<(), CacheError>;
    fn remove(&self, key: &str) -> Result<(), CacheError>;
    fn clear(&self) -> Result<(), CacheError>;
}

/// Trait cache phân tán (Redis, Memcached, ...).
#[async_trait::async_trait]
pub trait DistributedCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    async fn get(&self, key: &str) -> Result<Option<T>, CacheError>;
    async fn set(&self, key: &str, value: T, ttl: Option<Duration>) -> Result<(), CacheError>;
    async fn remove(&self, key: &str) -> Result<(), CacheError>;
    async fn clear(&self) -> Result<(), CacheError>;
}

/// Memory cache đơn giản dùng DashMap (sharded map) để giảm contention khi số lượng key lớn.
#[derive(Clone)]
pub struct SimpleMemoryCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    data: Arc<DashMap<String, (T, Option<SystemTime>)>>,
}

impl<T> SimpleMemoryCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self {
            data: Arc::new(DashMap::new()),
        }
    }
}

impl<T> MemoryCache<T> for SimpleMemoryCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn get(&self, key: &str) -> Result<Option<T>, CacheError> {
        if let Some(entry) = self.data.get(key) {
            if let Some(expiry_time) = entry.1 {
                if SystemTime::now() > expiry_time {
                    drop(entry);
                    self.data.remove(key);
                    return Ok(None);
                }
            }
            Ok(Some(entry.0.clone()))
        } else {
            Ok(None)
        }
    }

    fn set(&self, key: &str, value: T, ttl: Option<Duration>) -> Result<(), CacheError> {
        let expiry = ttl.map(|d| SystemTime::now() + d);
        self.data.insert(key.to_string(), (value, expiry));
        Ok(())
    }

    fn remove(&self, key: &str) -> Result<(), CacheError> {
        self.data.remove(key);
        Ok(())
    }

    fn clear(&self) -> Result<(), CacheError> {
        self.data.clear();
        Ok(())
    }
}

impl<T> Default for SimpleMemoryCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Placeholder cho distributed cache (chưa implement Redis/Memcached).
pub struct PlaceholderDistributedCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    _phantom: std::marker::PhantomData<T>,
}

impl<T> PlaceholderDistributedCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<T> DistributedCache<T> for PlaceholderDistributedCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    async fn get(&self, _key: &str) -> Result<Option<T>, CacheError> {
        // TODO: Implement actual distributed cache lookup
        // Placeholder: always return None (cache miss)
        Ok(None)
    }

    async fn set(&self, _key: &str, _value: T, _ttl: Option<Duration>) -> Result<(), CacheError> {
        // TODO: Implement actual distributed cache storage
        // Placeholder: do nothing
        Ok(())
    }

    async fn remove(&self, _key: &str) -> Result<(), CacheError> {
        // TODO: Implement actual distributed cache removal
        // Placeholder: do nothing
        Ok(())
    }

    async fn clear(&self) -> Result<(), CacheError> {
        // TODO: Implement actual distributed cache clear
        // Placeholder: do nothing
        Ok(())
    }
}

impl<T> Default for PlaceholderDistributedCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}
