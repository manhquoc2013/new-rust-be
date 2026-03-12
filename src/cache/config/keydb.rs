//! Client Redis/KeyDB: get/set/del async, reconnect, serialize JSON.
//! KEYDB_POOL_SIZE (default 1): number of connections for parallel get/set when cache load is high.

use redis::{aio::MultiplexedConnection, AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;
use tokio::{
    sync::RwLock,
    time::{sleep, Duration},
};

#[derive(Clone)]
pub struct KeyDB {
    #[allow(dead_code)]
    client: Client,
    /// One slot per connection; round-robin for parallel get/set.
    pool: Arc<Vec<Arc<RwLock<Option<MultiplexedConnection>>>>>,
    pool_index: Arc<AtomicUsize>,
}

impl KeyDB {
    pub async fn new(url: &str) -> Arc<Self> {
        let client = match Client::open(url) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(url = %url, error = %e, "[Cache] Invalid KeyDB URL");
                Client::open("redis://127.0.0.1/").unwrap()
            }
        };
        let pool_size = std::env::var("KEYDB_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1)
            .clamp(1, 64);
        let mut pool = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            pool.push(Arc::new(RwLock::new(None)));
        }
        let pool = Arc::new(pool);
        let keydb = Arc::new(KeyDB {
            client: client.clone(),
            pool: pool.clone(),
            pool_index: Arc::new(AtomicUsize::new(0)),
        });
        for i in 0..pool_size {
            let slot = pool[i].clone();
            let client = client.clone();
            tokio::spawn(async move {
                loop {
                    match client.get_multiplexed_async_connection().await {
                        Ok(conn) => {
                            {
                                let mut guard = slot.write().await;
                                *guard = Some(conn);
                            }
                            tracing::info!(slot = i, pool_size, "[Cache] KeyDB slot connected");
                            break;
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, slot = i, "[Cache] KeyDB connect failed, retrying in 5s");
                            sleep(Duration::from_secs(5)).await;
                        }
                    }
                }
                let slot2 = slot.clone();
                let client2 = client.clone();
                loop {
                    sleep(Duration::from_secs(10)).await;
                    let alive = {
                        let mut conn_guard = slot2.write().await;
                        if let Some(conn) = conn_guard.as_mut() {
                            let pong: redis::RedisResult<String> =
                                redis::cmd("PING").query_async(conn).await;
                            matches!(pong, Ok(ref resp) if resp == "PONG")
                        } else {
                            false
                        }
                    };
                    if !alive {
                        tracing::warn!(slot = i, "[Cache] KeyDB connection lost, reconnecting");
                        {
                            let mut guard = slot2.write().await;
                            *guard = None;
                        }
                        loop {
                            match client2.get_multiplexed_async_connection().await {
                                Ok(conn) => {
                                    {
                                        let mut guard = slot2.write().await;
                                        *guard = Some(conn);
                                    }
                                    tracing::info!(slot = i, "[Cache] KeyDB reconnected");
                                    break;
                                }
                                Err(err) => {
                                    tracing::warn!(error = %err, slot = i, "[Cache] KeyDB reconnect failed, retrying in 5s");
                                    sleep(Duration::from_secs(5)).await;
                                }
                            }
                        }
                    }
                }
            });
        }
        keydb
    }

    /// Pick a connection slot (round-robin) for parallel get/set.
    fn slot(&self) -> Arc<RwLock<Option<MultiplexedConnection>>> {
        let idx = self.pool_index.fetch_add(1, AtomicOrdering::Relaxed) % self.pool.len();
        self.pool[idx].clone()
    }

    /// Returns true if at least one slot is connected.
    pub async fn is_connected(&self) -> bool {
        for slot in self.pool.as_ref() {
            if slot.read().await.is_some() {
                return true;
            }
        }
        false
    }

    /// Ghi key (JSON). Returns Ok(()) on success or no connection (skip); Err on write failure after retry.
    /// For caller that needs result (vd. db_retry enqueue). One retry on transient error (IoError/ConnectionRefused).
    pub async fn set_result<T: Serialize>(&self, key: &str, value: &T) -> redis::RedisResult<()> {
        let json = serde_json::to_string(value).map_err(|e| {
            redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "KeyDB Serde serialize",
                e.to_string(),
            ))
        })?;
        let slot = self.slot();
        let mut guard = slot.write().await;
        let conn = match guard.as_mut() {
            Some(c) => c,
            None => {
                return Err(redis::RedisError::from((
                    redis::ErrorKind::IoError,
                    "KeyDB not connected",
                )));
            }
        };
        let mut last_err = match conn.set::<&str, String, ()>(key, json.clone()).await {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };
        drop(guard);
        // Retry once on transient error (network/timeout)
        let retry = last_err.is_io_error()
            || last_err.is_connection_refusal()
            || last_err.is_timeout()
            || last_err.is_connection_dropped();
        if retry {
            sleep(Duration::from_millis(200)).await;
            let slot = self.slot();
            let mut guard = slot.write().await;
            if let Some(conn) = guard.as_mut() {
                last_err = match conn.set::<&str, String, ()>(key, json).await {
                    Ok(()) => return Ok(()),
                    Err(e) => e,
                };
            }
        }
        Err(last_err)
    }

    pub async fn set<T: Serialize>(&self, key: &str, value: &T) {
        match self.set_result(key, value).await {
            Ok(()) => {}
            Err(e) => {
                if e.kind() == redis::ErrorKind::IoError && e.to_string().contains("not connected")
                {
                    tracing::debug!(key = %key, "[Cache] KeyDB not connected, skip set");
                } else {
                    tracing::error!(key = %key, error = %e, "[Cache] Redis set error");
                }
            }
        }
    }

    /// Ghi nhiều key trong một round-trip (pipeline SET). Dùng cho atomic_reload_prefix to reduce round-trips to KeyDB.
    /// One retry on transient error (IoError/timeout/connection dropped).
    /// Phiên bản trả Result để caller (vd. db_retry flush) biết thành công hay thất bại.
    pub async fn set_many_result<T: Serialize>(
        &self,
        pairs: &[(String, T)],
    ) -> redis::RedisResult<()> {
        if pairs.is_empty() {
            return Ok(());
        }
        let mut json_pairs: Vec<(String, String)> = Vec::with_capacity(pairs.len());
        for (key, value) in pairs {
            let json = serde_json::to_string(value).map_err(|e| {
                redis::RedisError::from((
                    redis::ErrorKind::TypeError,
                    "KeyDB Serde serialize",
                    e.to_string(),
                ))
            })?;
            json_pairs.push((key.clone(), json));
        }
        let slot = self.slot();
        let mut guard = slot.write().await;
        let conn = guard.as_mut().ok_or_else(|| {
            redis::RedisError::from((redis::ErrorKind::IoError, "KeyDB not connected"))
        })?;
        let mut pipe = redis::pipe();
        for (key, json) in &json_pairs {
            pipe.cmd("SET")
                .arg(key.as_str())
                .arg(json.as_str())
                .ignore();
        }
        pipe.query_async::<()>(conn).await?;
        Ok(())
    }

    /// Ghi nhiều key trong một round-trip (pipeline SET). Dùng cho atomic_reload_prefix to reduce round-trips to KeyDB.
    /// One retry on transient error (IoError/timeout/connection dropped).
    pub async fn set_many<T: Serialize>(&self, pairs: &[(String, T)]) {
        if pairs.is_empty() {
            return;
        }
        let mut json_pairs: Vec<(String, String)> = Vec::with_capacity(pairs.len());
        for (key, value) in pairs {
            match serde_json::to_string(value) {
                Ok(json) => json_pairs.push((key.clone(), json)),
                Err(e) => {
                    tracing::error!(key = %key, error = %e, "[Cache] KeyDB Serde serialize error in set_many");
                }
            }
        }
        if json_pairs.is_empty() {
            return;
        }
        let slot = self.slot();
        let mut guard = slot.write().await;
        let conn = match guard.as_mut() {
            Some(c) => c,
            None => {
                tracing::debug!(
                    count = json_pairs.len(),
                    "[Cache] KeyDB not connected, skip set_many"
                );
                return;
            }
        };
        let mut pipe = redis::pipe();
        for (key, json) in &json_pairs {
            pipe.cmd("SET")
                .arg(key.as_str())
                .arg(json.as_str())
                .ignore();
        }
        let last_err = match pipe.query_async::<()>(conn).await {
            Ok(()) => return,
            Err(e) => e,
        };
        drop(guard);
        let retry = last_err.is_io_error()
            || last_err.is_connection_refusal()
            || last_err.is_timeout()
            || last_err.is_connection_dropped();
        if retry {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let slot = self.slot();
            let mut guard = slot.write().await;
            if let Some(conn) = guard.as_mut() {
                let mut pipe2 = redis::pipe();
                for (key, json) in &json_pairs {
                    pipe2
                        .cmd("SET")
                        .arg(key.as_str())
                        .arg(json.as_str())
                        .ignore();
                }
                if let Err(e) = pipe2.query_async::<()>(conn).await {
                    tracing::error!(
                        count = json_pairs.len(),
                        error = %e,
                        "[Cache] KeyDB pipeline set_many failed after retry"
                    );
                }
            } else {
                tracing::debug!(
                    count = json_pairs.len(),
                    "[Cache] KeyDB not connected on set_many retry, skip"
                );
            }
        } else {
            tracing::error!(
                count = json_pairs.len(),
                error = %last_err,
                "[Cache] KeyDB pipeline set_many failed"
            );
        }
    }

    /// Ghi key chỉ khi chưa tồn tại (NX), kèm TTL giây. Dùng cho leader lock. Returns true if written.
    pub async fn set_nx_ex(
        &self,
        key: &str,
        value: &str,
        ttl_secs: u64,
    ) -> redis::RedisResult<bool> {
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            let result: Option<String> = redis::cmd("SET")
                .arg(key)
                .arg(value)
                .arg("NX")
                .arg("EX")
                .arg(ttl_secs)
                .query_async(conn)
                .await?;
            Ok(result.is_some())
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "No connection",
            )))
        }
    }

    /// Ghi đè key và set TTL (giây). Used to renew leader lock.
    pub async fn set_ex(&self, key: &str, value: &str, ttl_secs: u64) -> redis::RedisResult<()> {
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            let _: () = redis::cmd("SET")
                .arg(key)
                .arg(value)
                .arg("EX")
                .arg(ttl_secs)
                .query_async(conn)
                .await?;
            Ok(())
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "No connection",
            )))
        }
    }

    pub async fn get<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            match conn.get::<&str, Option<String>>(key).await {
                Ok(Some(json)) => match serde_json::from_str::<T>(&json) {
                    Ok(obj) => Some(obj),
                    Err(e) => {
                        tracing::error!(error = %e, "[Cache] KeyDB Serde deserialize error");
                        None
                    }
                },
                Ok(None) => None,
                Err(e) => {
                    tracing::error!(error = %e, "[Cache] KeyDB Redis get error");
                    None
                }
            }
        } else {
            tracing::debug!("[Cache] KeyDB not connected, skip get");
            None
        }
    }

    /// Lấy nhiều key trong một round-trip (MGET). Returns Vec<Option<T>> in same order as keys.
    pub async fn get_many<T: for<'de> Deserialize<'de>>(
        &self,
        keys: &[String],
    ) -> redis::RedisResult<Vec<Option<T>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            let mut cmd = redis::cmd("MGET");
            for key in keys {
                cmd.arg(key);
            }
            let raw: Vec<Option<String>> = cmd.query_async(conn).await?;
            let out: Vec<Option<T>> = raw
                .into_iter()
                .map(|opt| opt.and_then(|json| serde_json::from_str::<T>(&json).ok()))
                .collect();
            Ok(out)
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "No connection",
            )))
        }
    }
    pub async fn remove(&self, key: &str) -> redis::RedisResult<()> {
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            conn.del(key).await
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "No connection",
            )))
        }
    }

    /// Xóa nhiều key trong một round-trip (DEL key1 key2 ...). Returns number of keys deleted.
    pub async fn remove_many(&self, keys: &[String]) -> redis::RedisResult<u64> {
        if keys.is_empty() {
            return Ok(0);
        }
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            let n = conn.del::<_, u64>(keys).await?;
            Ok(n)
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "No connection",
            )))
        }
    }

    pub async fn remove_prefix(&self, prefix: &str) -> redis::RedisResult<()> {
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            let pattern = format!("{}*", prefix);
            let mut cursor: u64 = 0;
            let mut deleted = 0;
            loop {
                let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                    .cursor_arg(cursor)
                    .arg("MATCH")
                    .arg(&pattern)
                    .arg("COUNT")
                    .arg(1000)
                    .query_async(conn)
                    .await?;
                if !keys.is_empty() {
                    deleted += conn.del::<_, u64>(&keys).await?;
                }
                if next_cursor == 0 {
                    break;
                }
                cursor = next_cursor;
            }
            tracing::info!(deleted, prefix = %prefix, "[Cache] KeyDB removed keys by prefix");
            Ok(())
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "No connection",
            )))
        }
    }
    pub async fn exists(&self, key: &str) -> bool {
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            match conn.exists::<&str, bool>(key).await {
                Ok(exists) => exists,
                Err(e) => {
                    tracing::error!(key = %key, error = %e, "[Cache] KeyDB exists error");
                    false
                }
            }
        } else {
            tracing::debug!(key = %key, "[Cache] KeyDB not connected, skip exists");
            false
        }
    }

    pub async fn get_keys_with_prefix(&self, prefix: &str) -> redis::RedisResult<Vec<String>> {
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            let pattern = format!("{}*", prefix);
            let mut cursor: u64 = 0;
            let mut keys = Vec::new();
            loop {
                let (next_cursor, batch_keys): (u64, Vec<String>) = redis::cmd("SCAN")
                    .cursor_arg(cursor)
                    .arg("MATCH")
                    .arg(&pattern)
                    .arg("COUNT")
                    .arg(1000)
                    .query_async(conn)
                    .await?;
                keys.extend(batch_keys);
                if next_cursor == 0 {
                    break;
                }
                cursor = next_cursor;
            }
            Ok(keys)
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "No connection",
            )))
        }
    }

    #[allow(dead_code)]
    pub async fn rpush(&self, key: &str, value: &str) -> redis::RedisResult<()> {
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            conn.rpush::<_, _, ()>(key, value).await
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "No connection",
            )))
        }
    }

    /// RPUSH + giới hạn độ dài list (LTRIM giữ N phần tử mới nhất) + TTL cho key.
    /// Avoid unbounded list growth and keys living forever wasting resources.
    /// max_len: giữ tối đa bao nhiêu phần tử (cũ nhất bị cắt bỏ). ttl_secs: thời gian sống của key (giây).
    #[allow(dead_code)]
    pub async fn rpush_trim_ttl(
        &self,
        key: &str,
        value: &str,
        max_len: usize,
        ttl_secs: u64,
    ) -> redis::RedisResult<()> {
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            conn.rpush::<_, _, ()>(key, value).await?;
            if max_len > 0 {
                // Keep elements from -max_len to -1 (i.e. N newest elements)
                let _: () = redis::cmd("LTRIM")
                    .arg(key)
                    .arg(-(max_len as i64))
                    .arg(-1)
                    .query_async(conn)
                    .await?;
            }
            let _: () = redis::cmd("EXPIRE")
                .arg(key)
                .arg(ttl_secs)
                .query_async(conn)
                .await?;
            Ok(())
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "No connection",
            )))
        }
    }

    /// RPUSH nhiều phần tử trong một lệnh + LTRIM + EXPIRE. Tối ưu round-trip khi persist batch.
    /// values rỗng thì không gọi Redis. max_len/ttl_secs giống rpush_trim_ttl.
    pub async fn rpush_many_trim_ttl(
        &self,
        key: &str,
        values: &[String],
        max_len: usize,
        ttl_secs: u64,
    ) -> redis::RedisResult<()> {
        if values.is_empty() {
            return Ok(());
        }
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            conn.rpush::<_, _, ()>(key, values).await?;
            if max_len > 0 {
                let _: () = redis::cmd("LTRIM")
                    .arg(key)
                    .arg(-(max_len as i64))
                    .arg(-1)
                    .query_async(conn)
                    .await?;
            }
            let _: () = redis::cmd("EXPIRE")
                .arg(key)
                .arg(ttl_secs)
                .query_async(conn)
                .await?;
            Ok(())
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "No connection",
            )))
        }
    }

    #[allow(dead_code)]
    pub async fn lpop(&self, key: &str) -> redis::RedisResult<Option<String>> {
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            conn.lpop(key, None).await
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "No connection",
            )))
        }
    }

    pub async fn lpop_n(&self, key: &str, n: usize) -> redis::RedisResult<Vec<String>> {
        if n == 0 {
            return Ok(Vec::new());
        }
        let slot = self.slot();
        let mut guard = slot.write().await;
        if let Some(conn) = guard.as_mut() {
            let count = std::num::NonZeroUsize::new(n).unwrap_or(std::num::NonZeroUsize::MIN);
            let out: Option<Vec<String>> = conn.lpop(key, Some(count)).await?;
            Ok(out.unwrap_or_default())
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::IoError,
                "No connection",
            )))
        }
    }
}
