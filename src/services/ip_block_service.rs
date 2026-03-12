//! IP block service: in-memory failure counter, block state stored only in Redis/KeyDB (not Moka).
//! Used when client connection failures exceed threshold (handshake/config/decrypt); block 15min by default, reset timer if still called while blocked.
//!
//! **System impact**: When Redis/KeyDB fails: `get` returns None → allow connection (fail-open); `set` only logs, no panic.
//! In-memory lock held only briefly, no await inside lock.
//!
//! **Exception safety**: RwLock poison → reuse inner map (no panic). Counter uses saturating_add;
//! timestamp uses saturating_add to avoid overflow when config is very large.
//!
//! **DDoS**: This mechanism **reduces load per IP** (after N failures → block), useful for attacks from few IPs.
//! **Not sufficient** for distributed DDoS: need rate limit/WAF at prior layer.

use crate::cache::config::cache_prefix::CachePrefix;
use crate::cache::config::keydb::KeyDB;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// IP block service: per-IP failure counter (in-memory) and block state only in Redis/KeyDB.
pub struct IpBlockService {
    keydb: Arc<KeyDB>,
    failure_counts: Arc<std::sync::RwLock<HashMap<IpAddr, u32>>>,
}

fn ip_block_key(ip: IpAddr) -> String {
    format!("{}:{}", CachePrefix::IpBlock.as_str(), ip)
}

impl IpBlockService {
    /// Create service with KeyDB (Redis); block state stored in Redis only, not via Moka.
    pub fn new(keydb: Arc<KeyDB>) -> Self {
        Self {
            keydb,
            failure_counts: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Check if IP is blocked in Redis/KeyDB. If still called while blocked, reset block duration.
    /// Returns true if IP is blocked (and timer was reset), false if connection is allowed.
    /// When Redis fails: get returns None → false (fail-open, do not block connection).
    pub async fn is_blocked(&self, ip: IpAddr, block_duration_secs: u64) -> bool {
        let key = ip_block_key(ip);
        let val: Option<u64> = self.keydb.get(&key).await;
        let Some(blocked_until_secs) = val else {
            return false;
        };
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now_secs < blocked_until_secs {
            let new_until = now_secs.saturating_add(block_duration_secs);
            self.keydb.set(&key, &new_until).await;
            tracing::debug!(
                ip = %ip,
                reset_until_secs = new_until,
                "[Network] IP still blocked, block timer reset"
            );
            true
        } else {
            let _ = self.keydb.remove(&key).await;
            false
        }
    }

    /// Record one connection failure; if threshold reached, write block to Redis/KeyDB.
    /// No panic: RwLock poison reuses inner map; Redis set only logs error.
    pub async fn record_failure(&self, ip: IpAddr, threshold: u32, block_duration_secs: u64) {
        if threshold == 0 {
            return;
        }
        let count = {
            let mut map = self
                .failure_counts
                .write()
                .unwrap_or_else(|e| e.into_inner());
            let c = map.entry(ip).or_insert(0);
            *c = c.saturating_add(1);
            let n = *c;
            if n >= threshold {
                map.remove(&ip);
            }
            n
        };
        if count >= threshold {
            let now_secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let blocked_until_secs = now_secs.saturating_add(block_duration_secs);
            let key = ip_block_key(ip);
            self.keydb.set(&key, &blocked_until_secs).await;
            tracing::warn!(
                ip = %ip,
                failure_count = count,
                block_secs = block_duration_secs,
                "[Network] IP blocked due to too many connection failures, state saved to Redis"
            );
        }
    }

    /// Record successful connection: clear in-memory failure counter for IP.
    /// No panic: on RwLock poison reuse inner map and still remove (best effort).
    pub fn record_success(&self, ip: IpAddr) {
        let _ = self
            .failure_counts
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&ip);
    }
}
