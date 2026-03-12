//! In-memory cache for toll stations and lanes (TOLL, TOLL_LANE), get_toll_with_fallback, get_toll_lanes_by_toll_id_with_fallback.
#![allow(dead_code)]

use crate::utils::timestamp_ms;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// TOLL - Toll station info
/// Maps to table RATING_OWNER.TOLL
#[allow(clippy::upper_case_acronyms)]
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct TOLL {
    pub toll_id: i32,                 // NUMBER(10,0) - Primary Key
    pub toll_name: String,            // VARCHAR2(100)
    pub address: String,              // VARCHAR2(100)
    pub province: String,             // VARCHAR2(100)
    pub district: String,             // VARCHAR2(100)
    pub precinct: String,             // VARCHAR2(100)
    pub toll_type: String,            // VARCHAR2(2)
    pub tell_number: String,          // VARCHAR2(15)
    pub fax: String,                  // VARCHAR2(15)
    pub status: String,               // VARCHAR2(1)
    pub toll_code: String,            // VARCHAR2(20)
    pub toll_name_search: String,     // VARCHAR2(100)
    pub rate_type_parking: String,    // VARCHAR2(2)
    pub status_commercial: String,    // VARCHAR2(1)
    pub calculation_type: String,     // VARCHAR2(1) DEFAULT 'N'
    pub tcoc_version_id: Option<i32>, // NUMBER(10,0)
    pub syntax_code: String,          // VARCHAR2(50)
    pub vehicle_type_profile: String, // VARCHAR2(20)
    pub allow_same_inout: String,     // VARCHAR2(1) DEFAULT '0'
    pub allow_same_inout_time: i64,   // NUMBER(9,0) DEFAULT 0
    pub latch_hour: Option<String>,   // DATE — read via TO_CHAR as 'YYYY-MM-DD HH24:MI:SS'
    pub boo: String,                  // VARCHAR2(5) DEFAULT '1'
    pub close_time_code: String,      // VARCHAR2(10)
    pub calc_type: String,            // VARCHAR2(20)
    pub sequence_type: String,        // VARCHAR2(30) DEFAULT 'INT'
    pub sequence_apply: String,       // VARCHAR2(20) DEFAULT 'ALL'
}

impl fmt::Debug for TOLL {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TOLL")
            .field("toll_id", &self.toll_id)
            .field("toll_name", &self.toll_name)
            .field("toll_code", &self.toll_code)
            .field("toll_type", &self.toll_type)
            .field("status", &self.status)
            .field("boo", &self.boo)
            .field("address", &self.address)
            .field("province", &self.province)
            .field("district", &self.district)
            .field("precinct", &self.precinct)
            .field("calculation_type", &self.calculation_type)
            .field("vehicle_type_profile", &self.vehicle_type_profile)
            .field("allow_same_inout", &self.allow_same_inout)
            .field("allow_same_inout_time", &self.allow_same_inout_time)
            .field("sequence_type", &self.sequence_type)
            .field("sequence_apply", &self.sequence_apply)
            .finish()
    }
}

/// TOLL_LANE - Toll lane info
/// Maps to table RATING_OWNER.TOLL_LANE
#[derive(Default, Clone)]
pub struct TOLL_LANE {
    pub toll_id: i32,                  // NUMBER - Foreign Key -> TOLL.toll_id
    pub lane_code: i32,                // NUMBER - Lane code
    pub lane_type: String,             // VARCHAR2(2) - Lane type
    pub status: String,                // VARCHAR2(1) - Status
    pub toll_lane_id: i32,             // NUMBER - Primary Key
    pub lane_name: String,             // VARCHAR2(100) - Lane name
    pub free_lanes: String,            // VARCHAR2(400)
    pub free_lanes_limit: Option<i32>, // NUMBER(5,0)
    pub free_lanes_second: String,     // VARCHAR2(50)
    pub free_toll_id: Option<i32>,     // NUMBER
    pub dup_filter: String,            // VARCHAR2(1) DEFAULT 'N'
    pub free_limit_time: String,       // VARCHAR2(5)
    pub free_allow_loop: String,       // VARCHAR2(2) DEFAULT 'N'
    pub transit_lane: String,          // VARCHAR2(20) DEFAULT 'N'
    pub freeflow_lane: String,         // VARCHAR2(20) DEFAULT 'N'
}

impl fmt::Debug for TOLL_LANE {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TOLL_LANE")
            .field("toll_lane_id", &self.toll_lane_id)
            .field("toll_id", &self.toll_id)
            .field("lane_code", &self.lane_code)
            .field("lane_name", &self.lane_name)
            .field("lane_type", &self.lane_type)
            .field("status", &self.status)
            .field("free_lanes", &self.free_lanes)
            .field("free_lanes_limit", &self.free_lanes_limit)
            .field("free_lanes_second", &self.free_lanes_second)
            .field("free_toll_id", &self.free_toll_id)
            .field("dup_filter", &self.dup_filter)
            .field("free_limit_time", &self.free_limit_time)
            .field("free_allow_loop", &self.free_allow_loop)
            .field("transit_lane", &self.transit_lane)
            .field("freeflow_lane", &self.freeflow_lane)
            .finish()
    }
}

/// Cache entry for TOLL. last_access/access_count use atomic so get() does not need get_mut.
struct TollCacheEntry {
    toll: TOLL,
    created_at: i64,
    last_access: AtomicI64,
    access_count: AtomicU64,
}

impl Clone for TollCacheEntry {
    fn clone(&self) -> Self {
        Self {
            toll: self.toll.clone(),
            created_at: self.created_at,
            last_access: AtomicI64::new(self.last_access.load(Ordering::Relaxed)),
            access_count: AtomicU64::new(self.access_count.load(Ordering::Relaxed)),
        }
    }
}

/// Cache entry for TOLL_LANE. last_access/access_count use atomic.
struct TollLaneCacheEntry {
    lane: TOLL_LANE,
    created_at: i64,
    last_access: AtomicI64,
    access_count: AtomicU64,
}

impl Clone for TollLaneCacheEntry {
    fn clone(&self) -> Self {
        Self {
            lane: self.lane.clone(),
            created_at: self.created_at,
            last_access: AtomicI64::new(self.last_access.load(Ordering::Relaxed)),
            access_count: AtomicU64::new(self.access_count.load(Ordering::Relaxed)),
        }
    }
}

/// Cache statistics for TOLL (returned by get_stats; internal counters are atomic).
#[derive(Default, Clone, Debug)]
pub struct TollCacheStats {
    pub total_entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

/// Internal atomic counters to avoid locking on read path.
struct TollCacheStatsInner {
    total_entries: AtomicUsize,
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
}

impl Default for TollCacheStatsInner {
    fn default() -> Self {
        Self {
            total_entries: AtomicUsize::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }
}

/// Cache statistics for TOLL_LANE (returned by get_stats; internal counters are atomic).
#[derive(Default, Clone, Debug)]
pub struct TollLaneCacheStats {
    pub total_entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

/// Internal atomic counters for TollLaneCache.
struct TollLaneCacheStatsInner {
    total_entries: AtomicUsize,
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
}

impl Default for TollLaneCacheStatsInner {
    fn default() -> Self {
        Self {
            total_entries: AtomicUsize::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }
}

/// TOLL Memory Cache
/// Simple cache: toll_id -> TOLL
#[derive(Clone)]
pub struct TollCache {
    // Map: toll_id -> CacheEntry
    data: Arc<Mutex<HashMap<i32, TollCacheEntry>>>,
    default_ttl_ms: i64,
    max_total_entries: usize,
    stats: Arc<TollCacheStatsInner>,
}

impl TollCache {
    /// Create new cache with default config
    pub fn new() -> Self {
        Self::with_config(3_600_000, 1000) // 1h TTL, 1000 total entries
    }

    /// Create cache with custom config
    pub fn with_config(default_ttl_ms: i64, max_total_entries: usize) -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
            default_ttl_ms,
            max_total_entries,
            stats: Arc::new(TollCacheStatsInner::default()),
        }
    }

    /// Save TOLL to cache
    pub fn put(&self, toll: TOLL) {
        let toll_id = toll.toll_id;
        let now = timestamp_ms();

        let mut data = self.data.lock().unwrap();

        // Create cache entry
        let entry = TollCacheEntry {
            toll: toll.clone(),
            created_at: now,
            last_access: AtomicI64::new(now),
            access_count: AtomicU64::new(0),
        };

        // Check if already exists
        let is_new = !data.contains_key(&toll_id);

        data.insert(toll_id, entry);

        // Check total entries and evict if needed
        if data.len() > self.max_total_entries {
            self.evict_if_needed(&mut data);
        }

        // Update stats
        if is_new {
            self.stats
                .total_entries
                .store(data.len(), Ordering::Relaxed);
        }
    }

    /// Get TOLL by toll_id (cache hit). Updates last_access/access_count via atomic.
    pub fn get(&self, toll_id: i32) -> Option<TOLL> {
        let now = timestamp_ms();

        let mut data = self.data.lock().unwrap();

        if let Some(entry) = data.get(&toll_id) {
            let age = now - entry.created_at;
            if age < self.default_ttl_ms {
                entry.last_access.store(now, Ordering::Relaxed);
                entry.access_count.fetch_add(1, Ordering::Relaxed);
                self.stats.hits.fetch_add(1, Ordering::Relaxed);
                return Some(entry.toll.clone());
            } else {
                data.remove(&toll_id);
                self.stats
                    .total_entries
                    .store(data.len(), Ordering::Relaxed);
            }
        }

        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Get TOLL by toll_code. Updates last_access/access_count via atomic.
    pub fn get_by_code(&self, toll_code: &str) -> Option<TOLL> {
        let now = timestamp_ms();

        let data = self.data.lock().unwrap();

        for entry in data.values() {
            let age = now - entry.created_at;
            if age < self.default_ttl_ms && entry.toll.toll_code == toll_code {
                entry.last_access.store(now, Ordering::Relaxed);
                entry.access_count.fetch_add(1, Ordering::Relaxed);
                self.stats.hits.fetch_add(1, Ordering::Relaxed);
                return Some(entry.toll.clone());
            }
        }

        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Get all TOLL still valid. Updates last_access/access_count via atomic.
    pub fn get_all(&self) -> Vec<TOLL> {
        let now = timestamp_ms();

        let mut data = self.data.lock().unwrap();
        let mut result = Vec::new();
        let mut expired_keys = Vec::new();

        for (key, entry) in data.iter() {
            let age = now - entry.created_at;
            if age < self.default_ttl_ms {
                entry.last_access.store(now, Ordering::Relaxed);
                entry.access_count.fetch_add(1, Ordering::Relaxed);
                result.push(entry.toll.clone());
            } else {
                expired_keys.push(*key);
            }
        }

        let expired_count = expired_keys.len();
        for key in expired_keys {
            data.remove(&key);
        }

        if expired_count > 0 {
            self.stats
                .total_entries
                .store(data.len(), Ordering::Relaxed);
        }

        result
    }

    /// Remove TOLL from cache
    pub fn remove(&self, toll_id: i32) -> bool {
        let mut data = self.data.lock().unwrap();
        if data.remove(&toll_id).is_some() {
            self.stats
                .total_entries
                .store(data.len(), Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Remove all expired entries
    pub fn cleanup_expired(&self) -> usize {
        let now = timestamp_ms();

        let mut data = self.data.lock().unwrap();
        let mut expired_keys = Vec::new();

        for (key, entry) in data.iter() {
            let age = now - entry.created_at;
            if age >= self.default_ttl_ms {
                expired_keys.push(*key);
            }
        }

        let count = expired_keys.len();
        for key in expired_keys {
            data.remove(&key);
        }

        if count > 0 {
            self.stats
                .total_entries
                .store(data.len(), Ordering::Relaxed);
        }

        count
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> TollCacheStats {
        let data = self.data.lock().unwrap();
        TollCacheStats {
            total_entries: data.len(),
            hits: self.stats.hits.load(Ordering::Relaxed),
            misses: self.stats.misses.load(Ordering::Relaxed),
            evictions: self.stats.evictions.load(Ordering::Relaxed),
        }
    }

    /// Clear all cache
    pub fn clear(&self) {
        let mut data = self.data.lock().unwrap();
        data.clear();
        self.stats.total_entries.store(0, Ordering::Relaxed);
    }

    /// Replace entire cache with new TOLL list (atomic swap).
    /// Build map outside lock then assign once to minimize lock hold, no impact on readers.
    pub fn replace_all(&self, tolls: Vec<TOLL>) {
        let now = timestamp_ms();
        let new_data: HashMap<i32, TollCacheEntry> = tolls
            .into_iter()
            .map(|toll| {
                let toll_id = toll.toll_id;
                let entry = TollCacheEntry {
                    toll,
                    created_at: now,
                    last_access: AtomicI64::new(now),
                    access_count: AtomicU64::new(0),
                };
                (toll_id, entry)
            })
            .collect();
        let len = new_data.len();
        let mut data = self.data.lock().unwrap();
        *data = new_data;
        self.stats.total_entries.store(len, Ordering::Relaxed);
    }

    /// Evict entries if over max_total_entries (LRU)
    fn evict_if_needed(&self, data: &mut HashMap<i32, TollCacheEntry>) {
        if data.len() <= self.max_total_entries {
            return;
        }

        let mut entries: Vec<(i32, i64)> = data
            .iter()
            .map(|(key, entry)| (*key, entry.last_access.load(Ordering::Relaxed)))
            .collect();

        // Sort by last_access ascending (LRU first)
        entries.sort_by(|a, b| a.1.cmp(&b.1));

        // Remove oldest entries
        let to_remove = data.len() - self.max_total_entries;
        for i in 0..to_remove {
            if let Some((key, _)) = entries.get(i) {
                data.remove(key);
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.stats
            .total_entries
            .store(data.len(), Ordering::Relaxed);
    }
}

/// TOLL_LANE Memory Cache
/// Simple cache: toll_lane_id -> TOLL_LANE
#[derive(Clone)]
pub struct TollLaneCache {
    data: Arc<Mutex<HashMap<i32, TollLaneCacheEntry>>>,
    toll_lanes: Arc<Mutex<HashMap<i32, Vec<i32>>>>,
    default_ttl_ms: i64,
    max_total_entries: usize,
    stats: Arc<TollLaneCacheStatsInner>,
}

impl TollLaneCache {
    /// Create new cache with default config
    pub fn new() -> Self {
        Self::with_config(3_600_000, 5000) // 1h TTL, 5000 total entries
    }

    /// Create cache with custom config
    pub fn with_config(default_ttl_ms: i64, max_total_entries: usize) -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
            toll_lanes: Arc::new(Mutex::new(HashMap::new())),
            default_ttl_ms,
            max_total_entries,
            stats: Arc::new(TollLaneCacheStatsInner::default()),
        }
    }

    /// Save TOLL_LANE to cache
    pub fn put(&self, lane: TOLL_LANE) {
        let lane_id = lane.toll_lane_id;
        let toll_id = lane.toll_id;
        let now = timestamp_ms();

        let mut data = self.data.lock().unwrap();
        let mut toll_lanes = self.toll_lanes.lock().unwrap();

        let entry = TollLaneCacheEntry {
            lane: lane.clone(),
            created_at: now,
            last_access: AtomicI64::new(now),
            access_count: AtomicU64::new(0),
        };

        // Check if already exists
        let is_new = !data.contains_key(&lane_id);

        data.insert(lane_id, entry);

        // Update toll_lanes index
        toll_lanes
            .entry(toll_id)
            .or_insert_with(Default::default)
            .push(lane_id);

        // Check total entries and evict if needed
        if data.len() > self.max_total_entries {
            self.evict_if_needed(&mut data, &mut toll_lanes);
        }

        // Update stats
        if is_new {
            self.stats
                .total_entries
                .store(data.len(), Ordering::Relaxed);
        }
    }

    /// Get TOLL_LANE by toll_lane_id (cache hit). Updates last_access/access_count via atomic.
    pub fn get(&self, toll_lane_id: i32) -> Option<TOLL_LANE> {
        let now = timestamp_ms();

        let mut data = self.data.lock().unwrap();

        if let Some(entry) = data.get(&toll_lane_id) {
            let age = now - entry.created_at;
            if age < self.default_ttl_ms {
                entry.last_access.store(now, Ordering::Relaxed);
                entry.access_count.fetch_add(1, Ordering::Relaxed);
                self.stats.hits.fetch_add(1, Ordering::Relaxed);
                return Some(entry.lane.clone());
            } else {
                data.remove(&toll_lane_id);
                self.stats
                    .total_entries
                    .store(data.len(), Ordering::Relaxed);
            }
        }

        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Get all TOLL_LANE for a toll_id. Updates last_access/access_count via atomic.
    /// When removing expired entry from data also update toll_lanes index to avoid stale index.
    pub fn get_by_toll_id(&self, toll_id: i32) -> Vec<TOLL_LANE> {
        let now = timestamp_ms();

        let mut data = self.data.lock().unwrap();
        let mut toll_lanes = self.toll_lanes.lock().unwrap();

        let mut result = Vec::new();
        let mut expired_lane_ids = Vec::new();

        if let Some(lane_ids) = toll_lanes.get(&toll_id) {
            for lane_id in lane_ids {
                if let Some(entry) = data.get(lane_id) {
                    let age = now - entry.created_at;
                    if age < self.default_ttl_ms {
                        entry.last_access.store(now, Ordering::Relaxed);
                        entry.access_count.fetch_add(1, Ordering::Relaxed);
                        result.push(entry.lane.clone());
                        self.stats.hits.fetch_add(1, Ordering::Relaxed);
                    } else {
                        expired_lane_ids.push(*lane_id);
                    }
                }
            }
        }

        let expired_count = expired_lane_ids.len();
        for lane_id in expired_lane_ids {
            if let Some(entry) = data.remove(&lane_id) {
                if let Some(lanes) = toll_lanes.get_mut(&entry.lane.toll_id) {
                    lanes.retain(|&id| id != lane_id);
                    if lanes.is_empty() {
                        toll_lanes.remove(&entry.lane.toll_id);
                    }
                }
            }
        }

        if expired_count > 0 {
            self.stats
                .total_entries
                .store(data.len(), Ordering::Relaxed);
        }

        result
    }

    /// Remove TOLL_LANE from cache
    pub fn remove(&self, toll_lane_id: i32) -> bool {
        let mut data = self.data.lock().unwrap();
        let mut toll_lanes = self.toll_lanes.lock().unwrap();

        if let Some(entry) = data.remove(&toll_lane_id) {
            // Remove from toll_lanes index
            if let Some(lanes) = toll_lanes.get_mut(&entry.lane.toll_id) {
                lanes.retain(|&id| id != toll_lane_id);
                if lanes.is_empty() {
                    toll_lanes.remove(&entry.lane.toll_id);
                }
            }

            self.stats
                .total_entries
                .store(data.len(), Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Remove all expired entries
    pub fn cleanup_expired(&self) -> usize {
        let now = timestamp_ms();

        let mut data = self.data.lock().unwrap();
        let mut toll_lanes = self.toll_lanes.lock().unwrap();
        let mut expired_keys = Vec::new();

        for (key, entry) in data.iter() {
            let age = now - entry.created_at;
            if age >= self.default_ttl_ms {
                expired_keys.push(*key);
            }
        }

        let count = expired_keys.len();
        for key in expired_keys {
            if let Some(entry) = data.remove(&key) {
                // Remove from toll_lanes index
                if let Some(lanes) = toll_lanes.get_mut(&entry.lane.toll_id) {
                    lanes.retain(|&id| id != key);
                    if lanes.is_empty() {
                        toll_lanes.remove(&entry.lane.toll_id);
                    }
                }
            }
        }

        if count > 0 {
            self.stats
                .total_entries
                .store(data.len(), Ordering::Relaxed);
        }

        count
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> TollLaneCacheStats {
        let data = self.data.lock().unwrap();
        TollLaneCacheStats {
            total_entries: data.len(),
            hits: self.stats.hits.load(Ordering::Relaxed),
            misses: self.stats.misses.load(Ordering::Relaxed),
            evictions: self.stats.evictions.load(Ordering::Relaxed),
        }
    }

    /// Clear all cache
    pub fn clear(&self) {
        let mut data = self.data.lock().unwrap();
        let mut toll_lanes = self.toll_lanes.lock().unwrap();
        data.clear();
        toll_lanes.clear();
        self.stats.total_entries.store(0, Ordering::Relaxed);
    }

    /// Replace entire cache with new TOLL_LANE list (atomic swap).
    /// Build data and toll_lanes outside lock then assign once to minimize lock hold.
    pub fn replace_all(&self, lanes: Vec<TOLL_LANE>) {
        let now = timestamp_ms();
        let mut new_data = HashMap::new();
        let mut new_toll_lanes: HashMap<i32, Vec<i32>> = HashMap::new();
        for lane in lanes {
            let lane_id = lane.toll_lane_id;
            let toll_id = lane.toll_id;
            let entry = TollLaneCacheEntry {
                lane: lane.clone(),
                created_at: now,
                last_access: AtomicI64::new(now),
                access_count: AtomicU64::new(0),
            };
            new_data.insert(lane_id, entry);
            new_toll_lanes.entry(toll_id).or_default().push(lane_id);
        }
        let len = new_data.len();
        let mut data = self.data.lock().unwrap();
        let mut toll_lanes = self.toll_lanes.lock().unwrap();
        *data = new_data;
        *toll_lanes = new_toll_lanes;
        self.stats.total_entries.store(len, Ordering::Relaxed);
    }

    /// Evict entries if over max_total_entries (LRU)
    fn evict_if_needed(
        &self,
        data: &mut HashMap<i32, TollLaneCacheEntry>,
        toll_lanes: &mut HashMap<i32, Vec<i32>>,
    ) {
        if data.len() <= self.max_total_entries {
            return;
        }

        let mut entries: Vec<(i32, i64)> = data
            .iter()
            .map(|(key, entry)| (*key, entry.last_access.load(Ordering::Relaxed)))
            .collect();

        // Sort by last_access ascending (LRU first)
        entries.sort_by(|a, b| a.1.cmp(&b.1));

        // Remove oldest entries
        let to_remove = data.len() - self.max_total_entries;
        for i in 0..to_remove {
            if let Some((key, _)) = entries.get(i) {
                if let Some(entry) = data.remove(key) {
                    // Remove from toll_lanes index
                    if let Some(lanes) = toll_lanes.get_mut(&entry.lane.toll_id) {
                        lanes.retain(|&id| id != *key);
                        if lanes.is_empty() {
                            toll_lanes.remove(&entry.lane.toll_id);
                        }
                    }
                    self.stats.evictions.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        self.stats
            .total_entries
            .store(data.len(), Ordering::Relaxed);
    }
}

// ============================================================================
// Global cache instance
// ============================================================================

static TOLL_CACHE: OnceLock<TollCache> = OnceLock::new();
static TOLL_LANE_CACHE: OnceLock<TollLaneCache> = OnceLock::new();

/// Get global TOLL cache instance
pub fn get_toll_cache() -> &'static TollCache {
    TOLL_CACHE.get_or_init(TollCache::new)
}

/// Get global TOLL_LANE cache instance
pub fn get_toll_lane_cache() -> &'static TollLaneCache {
    TOLL_LANE_CACHE.get_or_init(TollLaneCache::new)
}

// ============================================================================
// Helper functions
// ============================================================================

/// Save TOLL to global cache
pub fn save_toll(toll: TOLL) {
    get_toll_cache().put(toll);
}

/// Get TOLL from global cache by toll_id
pub fn get_toll(toll_id: i32) -> Option<TOLL> {
    get_toll_cache().get(toll_id)
}

/// Get TOLL from global cache by toll_code
pub fn get_toll_by_code(toll_code: &str) -> Option<TOLL> {
    get_toll_cache().get_by_code(toll_code)
}

/// Get all TOLL from global cache
pub fn get_all_tolls() -> Vec<TOLL> {
    get_toll_cache().get_all()
}

/// Remove TOLL from global cache
pub fn remove_toll(toll_id: i32) -> bool {
    get_toll_cache().remove(toll_id)
}

/// Replace all TOLL in global cache (used on reload; atomic swap, minimal reader blocking).
pub fn replace_all_tolls(tolls: Vec<TOLL>) {
    get_toll_cache().replace_all(tolls);
}

/// Save TOLL_LANE to global cache
pub fn save_toll_lane(lane: TOLL_LANE) {
    get_toll_lane_cache().put(lane);
}

/// Get TOLL_LANE from global cache by toll_lane_id
pub fn get_toll_lane(toll_lane_id: i32) -> Option<TOLL_LANE> {
    get_toll_lane_cache().get(toll_lane_id)
}

/// Get all TOLL_LANE for a toll_id from global cache
pub fn get_toll_lanes_by_toll_id(toll_id: i32) -> Vec<TOLL_LANE> {
    get_toll_lane_cache().get_by_toll_id(toll_id)
}

/// Replace all TOLL_LANE in global cache (used on reload; atomic swap, minimal reader blocking).
pub fn replace_all_toll_lanes(lanes: Vec<TOLL_LANE>) {
    get_toll_lane_cache().replace_all(lanes);
}

/// Get all TOLL_LANE for a toll_id with fallback (in-memory cache → distributed cache → DB)
/// Fallback: try in-memory cache first, then query distributed cache/DB
pub async fn get_toll_lanes_by_toll_id_with_fallback(
    toll_id: i32,
    db_pool: std::sync::Arc<r2d2::Pool<crate::configs::pool_factory::OdbcConnectionManager>>,
    cache: std::sync::Arc<crate::cache::config::cache_manager::CacheManager>,
) -> Vec<TOLL_LANE> {
    // Step 1: Try in-memory cache first
    let cached_lanes = get_toll_lanes_by_toll_id(toll_id);
    if !cached_lanes.is_empty() {
        tracing::debug!(
            toll_id,
            lanes = cached_lanes.len(),
            "[Cache] TollCache toll_lanes in-memory hit"
        );
        return cached_lanes;
    }

    // Step 2: Cache miss - fallback to distributed cache/DB
    tracing::info!(
        toll_id,
        "[Cache] TollCache toll_lanes in-memory miss, fallback to distributed/DB"
    );

    use crate::cache::data::toll_lane_cache::get_toll_lanes_by_toll_id as get_from_distributed_cache;
    let lanes_dto = get_from_distributed_cache(db_pool, cache, toll_id).await;

    // Convert TollLaneDto to TOLL_LANE and save to in-memory cache
    lanes_dto
        .into_iter()
        .map(|dto| {
            let toll_lane = dto.to_toll_lane();
            save_toll_lane(toll_lane.clone());
            toll_lane
        })
        .collect()
}

/// Get TOLL by toll_id with fallback (in-memory → CacheManager → DB TOLL → derive TOLL from DB TOLL_LANE).
pub async fn get_toll_with_fallback(
    toll_id: i32,
    db_pool: std::sync::Arc<r2d2::Pool<crate::configs::pool_factory::OdbcConnectionManager>>,
    cache: std::sync::Arc<crate::cache::config::cache_manager::CacheManager>,
) -> Option<TOLL> {
    if let Some(toll) = get_toll(toll_id) {
        tracing::debug!(toll_id, "[Cache] TollCache toll in-memory hit");
        return Some(toll);
    }

    tracing::debug!(
        toll_id,
        "[Cache] TollCache toll in-memory miss, fallback to DB"
    );
    if let Ok(Some(toll)) =
        crate::cache::data::toll_cache::get_toll_from_db(db_pool.clone(), toll_id).await
    {
        let key = cache.gen_key(
            crate::cache::config::cache_prefix::CachePrefix::Toll,
            &[&toll_id],
        );
        cache.set(&key, &toll).await;
        save_toll(toll.clone());
        tracing::debug!(toll_id, "[Cache] TollCache TOLL loaded from DB and cached");
        return Some(toll);
    }

    tracing::info!(
        toll_id,
        "[Cache] TollCache TOLL not found in DB, fallback to lanes"
    );
    let lanes = get_toll_lanes_by_toll_id_with_fallback(toll_id, db_pool, cache.clone()).await;
    if lanes.is_empty() {
        tracing::warn!(
            toll_id,
            "[Cache] TollCache no TOLL_LANEs, TOLL not available"
        );
        return None;
    }
    let toll = TOLL {
        toll_id,
        toll_name: format!("Toll {}", toll_id),
        toll_code: toll_id.to_string(),
        status: "1".to_string(),
        ..Default::default()
    };
    let key = cache.gen_key(
        crate::cache::config::cache_prefix::CachePrefix::Toll,
        &[&toll_id],
    );
    cache.set(&key, &toll).await;
    save_toll(toll.clone());
    tracing::info!(
        toll_id,
        "[Cache] TollCache TOLL loaded from lanes and cached"
    );
    Some(toll)
}

/// Remove TOLL_LANE from global cache
pub fn remove_toll_lane(toll_lane_id: i32) -> bool {
    get_toll_lane_cache().remove(toll_lane_id)
}
