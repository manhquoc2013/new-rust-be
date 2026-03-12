//! Cache bộ nhớ in-process (moka::future::Cache), get/set/del bằng key string.
//! Cấu hình qua env: MOKA_MAX_CAPACITY (0 = không giới hạn), MOKA_TTL_SECS (0 = không TTL),
//! MOKA_INVALIDATE_PREFIX_MAX_KEYS (cap số key xóa/lấy mỗi lần theo prefix, mặc định 10_000).

use std::time::Duration;

use crate::constants::cache;
use moka::future::Cache;

#[derive(Clone)]
pub struct MokaCache {
    cache: Cache<String, String>, // JSON string
    prefix_op_max_keys: usize,
}

impl MokaCache {
    /// Tạo cache với cấu hình từ env (MOKA_MAX_CAPACITY, MOKA_TTL_SECS, MOKA_INVALIDATE_PREFIX_MAX_KEYS).
    pub fn new() -> Self {
        let max_capacity = std::env::var("MOKA_MAX_CAPACITY")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let ttl_secs = std::env::var("MOKA_TTL_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let prefix_op_max_keys = std::env::var("MOKA_INVALIDATE_PREFIX_MAX_KEYS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(cache::MOKA_PREFIX_OP_MAX_KEYS);

        let mut builder = Cache::builder();
        if max_capacity > 0 {
            builder = builder.max_capacity(max_capacity);
        }
        if ttl_secs > 0 {
            builder = builder.time_to_live(Duration::from_secs(ttl_secs));
        }
        let cache = builder.build();

        Self {
            cache,
            prefix_op_max_keys,
        }
    }

    pub async fn get<T>(&self, key: &str) -> Option<T>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let json = self.cache.get(key).await?;
        match serde_json::from_str(&json) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::error!(key = %key, error = %e, "[Cache] Moka deserialize error");
                None
            }
        }
    }

    pub async fn set<T>(&self, key: &str, value: &T)
    where
        T: serde::Serialize,
    {
        match serde_json::to_string(value) {
            Ok(json) => {
                self.cache.insert(key.to_string(), json).await;
            }
            Err(e) => {
                tracing::error!(key = %key, error = %e, "[Cache] Moka serialize error");
            }
        }
    }

    pub async fn invalidate(&self, key: &str) {
        self.cache.invalidate(key).await;
    }

    pub async fn invalidate_prefix(&self, prefix: &str) {
        self.invalidate_prefix_capped(prefix, self.prefix_op_max_keys)
            .await;
    }

    /// Xóa keys theo prefix, tối đa `max_keys` (tránh spike khi cache rất lớn).
    pub async fn invalidate_prefix_capped(&self, prefix: &str, max_keys: usize) {
        let mut removed = 0usize;

        for (key, _value) in self.cache.iter() {
            if key.starts_with(prefix) {
                self.cache.invalidate(key.as_str()).await;
                removed += 1;
                if removed >= max_keys {
                    tracing::warn!(
                        prefix = %prefix,
                        removed,
                        max_keys,
                        "[Cache] Moka invalidate_prefix capped"
                    );
                    break;
                }
            }
        }

        if removed > 0 {
            tracing::info!(prefix = %prefix, removed, "[Cache] Moka invalidate_prefix");
        }
    }

    pub fn exists(&self, key: &str) -> bool {
        self.cache.contains_key(key)
    }

    /// Get all keys with prefix, tối đa `self.prefix_op_max_keys` (tránh collect quá lớn).
    pub fn get_keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        self.get_keys_with_prefix_capped(prefix, self.prefix_op_max_keys)
    }

    /// Lấy keys với prefix, giới hạn số lượng.
    pub fn get_keys_with_prefix_capped(&self, prefix: &str, max_keys: usize) -> Vec<String> {
        let mut keys = Vec::new();
        for (key, _) in self.cache.iter() {
            if key.as_str().starts_with(prefix) {
                keys.push(key.as_str().to_string());
                if keys.len() >= max_keys {
                    tracing::debug!(
                        prefix = %prefix,
                        collected = keys.len(),
                        max_keys,
                        "[Cache] Moka get_keys_with_prefix capped"
                    );
                    break;
                }
            }
        }
        keys
    }
}
