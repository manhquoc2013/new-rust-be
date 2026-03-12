//! CacheManager: gộp Moka + KeyDB, get/set/invalidate theo prefix (toll, price, ...).
//! Luồng lấy dữ liệu: memory (Moka) -> KeyDB -> query DB. Không áp dụng cho ETDR.

use std::{
    fmt::{Display, Write},
    sync::Arc,
};

use crate::cache::config::cache_prefix::CachePrefix;
use crate::cache::config::keydb::KeyDB;
use crate::cache::config::moka_cache::MokaCache;
use crate::constants::cache;
use serde::de::DeserializeOwned;

pub struct CacheManager {
    moka: Arc<MokaCache>,
    keydb: Arc<KeyDB>,
}

impl CacheManager {
    pub fn new(moka: Arc<MokaCache>, keydb: Arc<KeyDB>) -> Self {
        Self { moka, keydb }
    }

    /// Get typed value (ưu tiên Moka → fallback KeyDB)
    #[allow(dead_code)]
    pub async fn get<T>(&self, key: &str) -> Option<T>
    where
        T: serde::Serialize + for<'de> serde::Deserialize<'de>,
    {
        if let Some(v) = self.moka.get::<T>(key).await {
            tracing::debug!(key = %key, "[Cache] Moka hit");
            return Some(v);
        }

        if let Some(v) = self.keydb.get::<T>(key).await {
            tracing::debug!(key = %key, "[Cache] KeyDB hit");
            // populate Moka cache
            self.moka.set(key, &v).await;
            return Some(v);
        }

        None
    }

    /// Set typed value (Moka + KeyDB). Khi load from DB thành công, gọi set/atomic_reload_prefix để cập nhật ngược vào KeyDB.
    pub async fn set<T>(&self, key: &str, value: &T)
    where
        T: serde::Serialize,
    {
        self.moka.set(key, value).await;
        self.keydb.set(key, value).await;
    }

    /// Exists check (moka trước)
    #[allow(dead_code)]
    pub async fn exists(&self, key: &str) -> bool {
        if self.moka.exists(key) {
            return true;
        }
        self.keydb.exists(key).await
    }
    #[allow(dead_code)]
    pub async fn remove(&self, key: &str) {
        self.moka.invalidate(key).await;
        self.keydb.remove(key).await.ok();
    }
    #[allow(dead_code)]
    pub async fn remove_prefix(&self, prefix: &str) -> redis::RedisResult<()> {
        self.moka.invalidate_prefix(prefix).await;
        self.keydb.remove_prefix(prefix).await?;
        Ok(())
    }
    #[allow(dead_code)]
    pub fn moka(&self) -> &MokaCache {
        &self.moka
    }

    /// KeyDB để đồng bộ ETDR hoặc dữ liệu khác.
    pub fn keydb(&self) -> Arc<KeyDB> {
        self.keydb.clone()
    }
    pub fn gen_key(&self, prefix: CachePrefix, parts: &[&dyn Display]) -> String {
        let mut s = String::with_capacity(64);
        s.push_str(prefix.as_str());

        for p in parts {
            s.push(':');
            write!(&mut s, "{}", p).unwrap();
        }

        s
    }

    /// Get all keys with prefix từ cả Moka và KeyDB
    pub async fn get_keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        let mut keys = std::collections::HashSet::new();

        // Lấy keys từ Moka
        let moka_keys = self.moka.get_keys_with_prefix(prefix);
        keys.extend(moka_keys);

        // Lấy keys từ KeyDB
        if let Ok(keydb_keys) = self.keydb.get_keys_with_prefix(prefix).await {
            keys.extend(keydb_keys);
        }

        keys.into_iter().collect()
    }

    /// Load toàn bộ cặp (key, value) với prefix từ KeyDB (1 SCAN + 1 MGET). Dùng khi khởi động để hâm nóng cache.
    /// Keys không deserialize được bỏ qua. Không áp dụng cho ETDR.
    pub async fn load_prefix_from_keydb<T>(&self, prefix: &str) -> Vec<(String, T)>
    where
        T: DeserializeOwned,
    {
        let keys = match self.keydb.get_keys_with_prefix(prefix).await {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!(prefix = %prefix, error = %e, "[Cache] KeyDB get_keys_with_prefix failed");
                return Vec::new();
            }
        };
        let total_keys = keys.len();
        if total_keys == 0 {
            return Vec::new();
        }
        let values: Vec<Option<T>> = match self.keydb.get_many::<T>(&keys).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(prefix = %prefix, error = %e, "[Cache] KeyDB MGET failed");
                return Vec::new();
            }
        };
        let result: Vec<(String, T)> = keys
            .into_iter()
            .zip(values)
            .filter_map(|(k, opt)| opt.map(|v| (k, v)))
            .collect();
        tracing::debug!(prefix = %prefix, loaded = result.len(), total_keys, "[Cache] KeyDB prefix loaded");
        result
    }

    /// Atomic reload: set dữ liệu mới vào Moka + KeyDB, xóa keys cũ không còn trong danh sách. Load DB thành công thì dùng hàm này để lưu toàn bộ cache vào KeyDB (khi DB lost có thể fallback KeyDB). KeyDB lỗi chỉ log, Moka đã ghi trước nên app vẫn chạy.
    /// KeyDB dùng pipeline set_many + remove_many. Moka write yields every 200 keys to avoid blocking. Stale key removal capped per call when cache is large.
    pub async fn atomic_reload_prefix<T>(&self, prefix: &str, new_data: Vec<(String, T)>)
    where
        T: serde::Serialize,
    {
        let new_keys: std::collections::HashSet<String> =
            new_data.iter().map(|(key, _)| key.clone()).collect();

        // Step 1: Write new data — Moka in loop with periodic yield; KeyDB one pipeline
        for (i, (key, value)) in new_data.iter().enumerate() {
            self.moka.set(key, value).await;
            if i > 0 && i % 200 == 0 {
                tokio::task::yield_now().await;
            }
        }
        self.keydb.set_many(&new_data).await;

        // Step 2: Get current keys with prefix
        let all_keys = self.get_keys_with_prefix(prefix).await;

        // Step 3: Remove stale keys (capped to avoid long block when cache is very large)
        let stale_keys: Vec<String> = all_keys
            .into_iter()
            .filter(|k| !new_keys.contains(k))
            .collect();
        let total_stale = stale_keys.len();
        let to_remove = stale_keys
            .into_iter()
            .take(cache::ATOMIC_RELOAD_STALE_CAP)
            .collect::<Vec<_>>();
        let removed_count = to_remove.len();
        if removed_count > 0 {
            for key in &to_remove {
                self.moka.invalidate(key).await;
            }
            if let Err(e) = self.keydb.remove_many(&to_remove).await {
                tracing::warn!(
                    removed = removed_count,
                    prefix = %prefix,
                    error = %e,
                    "[Cache] KeyDB remove_many failed"
                );
            } else {
                tracing::info!(removed = removed_count, prefix = %prefix, "[Cache] removed stale keys");
            }
            if total_stale > removed_count {
                tracing::debug!(
                    prefix = %prefix,
                    remaining = total_stale - removed_count,
                    cap = cache::ATOMIC_RELOAD_STALE_CAP,
                    "[Cache] stale keys capped, remainder removed on next reload"
                );
            }
        }
    }
}
