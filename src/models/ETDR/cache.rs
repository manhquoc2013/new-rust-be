//! ETDR cache: ETDRCache, save/load, KeyDB sync, cleanup.
//! Some APIs (get_stats, count, clear, update_etdr, ...) reserved for admin/monitoring, not yet used.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use crate::cache::config::keydb::KeyDB;
use crate::constants::etdr::{
    ETDR_RETRY_KEY_PREFIX, ETDR_TTL_MS, NO_CHECKOUT_MAX_AGE_MS, PENDING_TCD_KEY_PREFIX,
};
use crate::db::repositories::TransportTransStageTcd;
use crate::utils::{normalize_etag, timestamp_ms};

use super::ETDR;

/// Cache entry with metadata. last_access/access_count use atomic so read path does not need get_mut.
struct CacheEntry {
    etdr: ETDR,
    created_at: i64,
    last_access: AtomicI64,
    access_count: AtomicU64,
}

impl Clone for CacheEntry {
    fn clone(&self) -> Self {
        Self {
            etdr: self.etdr.clone(),
            created_at: self.created_at,
            last_access: AtomicI64::new(self.last_access.load(Ordering::Relaxed)),
            access_count: AtomicU64::new(self.access_count.load(Ordering::Relaxed)),
        }
    }
}

/// Cache statistics (entry count, hit, miss, eviction).
#[derive(Default, Clone, Debug)]
pub struct CacheStats {
    pub total_entries: usize,
    pub total_etags: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

/// ETDR Memory Cache
/// Organized as memory cache with TTL, LRU eviction, and memory limit.
/// Optimization: per-etag locking to avoid race when multiple threads update same etag.
#[derive(Clone)]
pub struct ETDRCache {
    // Map: etag -> Vec<CacheEntry> - store multiple records per etag
    data: Arc<Mutex<HashMap<String, Vec<CacheEntry>>>>,
    // Per-etag locks to serialize operations on same etag
    // Optimization: std::sync::RwLock for concurrent reads, serialized writes
    // Note: std::sync::RwLock (not tokio) because methods run in sync context
    etag_locks: Arc<Mutex<HashMap<String, Arc<RwLock<()>>>>>,
    // Cache config
    max_entries_per_etag: usize, // Max records per etag
    default_ttl_ms: i64,         // Default TTL (milliseconds)
    max_total_entries: usize,    // Max total entries in cache
    // Statistics
    stats: Arc<Mutex<CacheStats>>,
}

impl ETDRCache {
    /// Create new cache with default config (TTL 1h per ETDR_TTL_MS).
    pub fn new() -> Self {
        Self::with_config(100, ETDR_TTL_MS, 10000) // 100 records/etag, 1h TTL, 10k total entries
    }

    /// Create cache with custom config
    pub fn with_config(
        max_entries_per_etag: usize,
        default_ttl_ms: i64,
        max_total_entries: usize,
    ) -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
            etag_locks: Arc::new(Mutex::new(HashMap::new())),
            max_entries_per_etag,
            default_ttl_ms,
            max_total_entries,
            stats: Arc::new(Mutex::new(CacheStats::default())),
        }
    }

    /// Get or create lock for a given etag
    /// Key is normalized (trim + trim null) so same etag always shares same lock.
    fn get_etag_lock(&self, etag: &str) -> Arc<RwLock<()>> {
        let key = normalize_etag(etag);
        let mut locks = self.etag_locks.lock().unwrap();
        locks
            .entry(key)
            .or_insert_with(|| Arc::new(RwLock::new(())))
            .clone()
    }

    /// Save ETDR to cache.
    /// Compare by ticket_id only: if record with same ticket_id exists then update, else insert. List per etag sorted by ticket_id descending.
    pub fn put(&self, etdr: ETDR) {
        let etag_key = normalize_etag(&etdr.etag_id);
        let now = timestamp_ms();

        let etag_lock = self.get_etag_lock(&etag_key);
        let _write_guard = etag_lock.write().unwrap();

        let mut data = self.data.lock().unwrap();
        let records = data.entry(etag_key.clone()).or_default();

        let ticket_id = etdr.ticket_id;
        let mut found_existing = false;
        for entry in records.iter_mut() {
            if ticket_id != 0 && entry.etdr.ticket_id == ticket_id {
                entry.etdr = etdr.clone();
                entry.created_at = etdr.checkin_datetime;
                entry.last_access.store(now, Ordering::Relaxed);
                entry.access_count.fetch_add(1, Ordering::Relaxed);
                found_existing = true;
                break;
            }
        }

        if !found_existing {
            let entry = CacheEntry {
                etdr: etdr.clone(),
                created_at: etdr.checkin_datetime,
                last_access: AtomicI64::new(now),
                access_count: AtomicU64::new(0),
            };
            records.insert(0, entry);
        }

        // Sort by ticket_id descending (newest first), then created_at so get_latest returns correct latest transaction by id.
        records.sort_by(|a, b| {
            let ord = b.etdr.ticket_id.cmp(&a.etdr.ticket_id);
            if std::cmp::Ordering::Equal == ord {
                b.created_at.cmp(&a.created_at)
            } else {
                ord
            }
        });

        // Cap records per etag
        if records.len() > self.max_entries_per_etag {
            let removed = records.len() - self.max_entries_per_etag;
            records.truncate(self.max_entries_per_etag);

            // Update stats
            let mut stats = self.stats.lock().unwrap();
            stats.evictions += removed as u64;
        }

        // Check total entries and evict if needed
        self.evict_if_needed(&mut data);
    }

    /// Get latest transaction by etag: record with largest ticket_id still within TTL (list sorted by ticket_id descending).
    pub fn get_latest_checkin(&self, etag: &str) -> Option<ETDR> {
        let etag_key = normalize_etag(etag);
        let now = timestamp_ms();

        let etag_lock = self.get_etag_lock(&etag_key);
        let _read_guard = etag_lock.read().unwrap();

        let data = self.data.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();

        if let Some(records) = data.get(&etag_key) {
            for entry in records {
                if etdr_within_ttl(&entry.etdr) {
                    entry.last_access.store(now, Ordering::Relaxed);
                    entry.access_count.fetch_add(1, Ordering::Relaxed);
                    stats.hits += 1;
                    return Some(entry.etdr.clone());
                }
            }
        }

        stats.misses += 1;
        None
    }

    /// Get latest transaction by etag without checkout (toll_out == 0): largest ticket_id still within TTL.
    pub fn get_latest_checkin_without_checkout(&self, etag: &str) -> Option<ETDR> {
        let etag_key = normalize_etag(etag);
        let now = timestamp_ms();

        let etag_lock = self.get_etag_lock(&etag_key);
        let _read_guard = etag_lock.read().unwrap();

        let data = self.data.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();

        if let Some(records) = data.get(&etag_key) {
            for entry in records {
                if etdr_within_ttl(&entry.etdr) && entry.etdr.toll_out == 0 {
                    entry.last_access.store(now, Ordering::Relaxed);
                    entry.access_count.fetch_add(1, Ordering::Relaxed);
                    stats.hits += 1;
                    return Some(entry.etdr.clone());
                }
            }
        }

        stats.misses += 1;
        None
    }

    /// Update ETDR atomically with optimistic locking
    /// Key etag normalized.
    pub fn update_etdr(
        &self,
        etag: &str,
        ref_trans_id: i64,
        updater: impl FnOnce(&mut ETDR),
    ) -> bool {
        let etag_key = normalize_etag(etag);
        let etag_lock = self.get_etag_lock(&etag_key);
        let _write_guard = etag_lock.write().unwrap();

        let mut data = self.data.lock().unwrap();

        if let Some(records) = data.get_mut(&etag_key) {
            for entry in records.iter_mut() {
                if entry.etdr.ref_trans_id == ref_trans_id {
                    updater(&mut entry.etdr);
                    entry.last_access.store(timestamp_ms(), Ordering::Relaxed);
                    return true;
                }
            }
        }

        false
    }

    /// Get all ETDR for an etag that are still valid
    /// Key etag normalized.
    pub fn get_all_by_etag(&self, etag: &str) -> Vec<ETDR> {
        let etag_key = normalize_etag(etag);
        let now = timestamp_ms();

        let mut data = self.data.lock().unwrap();

        if let Some(records) = data.get_mut(&etag_key) {
            let mut result = Vec::new();
            for entry in records.iter_mut() {
                if etdr_within_ttl(&entry.etdr) {
                    entry.last_access.store(now, Ordering::Relaxed);
                    entry.access_count.fetch_add(1, Ordering::Relaxed);
                    result.push(entry.etdr.clone());
                }
            }
            result
        } else {
            Vec::new()
        }
    }

    /// Get ETDR by ref_trans_id
    pub fn get_by_ref_trans_id(&self, ref_trans_id: i64) -> Option<ETDR> {
        let now = timestamp_ms();

        let mut data = self.data.lock().unwrap();

        for records in data.values_mut() {
            for entry in records.iter_mut() {
                if etdr_within_ttl(&entry.etdr) && entry.etdr.ref_trans_id == ref_trans_id {
                    entry.last_access.store(now, Ordering::Relaxed);
                    entry.access_count.fetch_add(1, Ordering::Relaxed);
                    return Some(entry.etdr.clone());
                }
            }
        }
        None
    }

    /// Update hub_id for ETDR with matching request_id (from CHECKIN_HUB_INFO Kafka message).
    /// Returns true if found and updated, false if no ETDR with that request_id.
    /// Sync updated record to KeyDB (key = etdr:{etag_id}) if KeyDB is registered.
    pub fn update_hub_id_by_request_id(&self, request_id: i64, hub_id: i64) -> bool {
        let mut data = self.data.lock().unwrap();
        for records in data.values_mut() {
            for entry in records.iter_mut() {
                if entry.etdr.request_id == request_id {
                    entry.etdr.hub_id = Some(hub_id);
                    entry.last_access.store(timestamp_ms(), Ordering::Relaxed);
                    sync_etdr_to_keydb_if_set(&entry.etdr);
                    return true;
                }
            }
        }
        false
    }

    /// Update hub_id for ETDR with matching ticket_id (from CHECKIN_HUB_INFO Kafka message).
    /// Returns true if found and updated, false if no ETDR with that ticket_id.
    /// Sync updated record to KeyDB (key = etdr:{etag_id}) if KeyDB is registered.
    pub fn update_hub_id_by_ticket_id(&self, ticket_id: i64, hub_id: i64) -> bool {
        let mut data = self.data.lock().unwrap();
        for records in data.values_mut() {
            for entry in records.iter_mut() {
                if entry.etdr.ticket_id == ticket_id {
                    entry.etdr.hub_id = Some(hub_id);
                    entry.last_access.store(timestamp_ms(), Ordering::Relaxed);
                    sync_etdr_to_keydb_if_set(&entry.etdr);
                    return true;
                }
            }
        }
        false
    }

    /// Get transaction by ticket_id (each ETDR has a unique ticket_id).
    pub fn get_by_ticket_id(&self, ticket_id: i64) -> Option<ETDR> {
        let now = timestamp_ms();

        let mut data = self.data.lock().unwrap();

        for records in data.values_mut() {
            for entry in records.iter_mut() {
                if etdr_within_ttl(&entry.etdr) && entry.etdr.ticket_id == ticket_id {
                    entry.last_access.store(now, Ordering::Relaxed);
                    entry.access_count.fetch_add(1, Ordering::Relaxed);
                    return Some(entry.etdr.clone());
                }
            }
        }
        None
    }

    /// Get largest ticket_id (or boo_ticket_id) in cache, used as fallback when not available from DB.
    /// Returns None if cache empty or no valid entry.
    pub fn get_max_ticket_id(&self) -> Option<i64> {
        let data = self.data.lock().unwrap();
        let mut max_id: Option<i64> = None;
        for records in data.values() {
            for entry in records.iter() {
                if etdr_within_ttl(&entry.etdr) {
                    let tid = entry.etdr.ticket_id;
                    let boo_tid = entry.etdr.boo_ticket_id;
                    let current_max = tid.max(boo_tid);
                    max_id = Some(match max_id {
                        Some(m) => m.max(current_max),
                        None => current_max,
                    });
                }
            }
        }
        max_id
    }

    /// Evict entries if over max_total_entries
    fn evict_if_needed(&self, data: &mut HashMap<String, Vec<CacheEntry>>) {
        let total_entries: usize = data.values().map(|v| v.len()).sum();

        if total_entries > self.max_total_entries {
            // Sort all entries by last_access (LRU)
            let mut all_entries: Vec<(String, CacheEntry)> = Vec::new();
            for (etag, records) in data.iter() {
                for entry in records.iter() {
                    all_entries.push((etag.clone(), entry.clone()));
                }
            }

            // Sort by last_access ascending (least used first)
            all_entries.sort_by(|a, b| {
                a.1.last_access
                    .load(Ordering::Relaxed)
                    .cmp(&b.1.last_access.load(Ordering::Relaxed))
            });

            // Remove least-used entries
            let to_remove = total_entries - self.max_total_entries;
            let mut stats = self.stats.lock().unwrap();

            // Group by etag and remove by ticket_id (unique id)
            let mut to_remove_by_etag: HashMap<String, Vec<i64>> = HashMap::new();
            for (etag, entry) in all_entries.iter().take(to_remove) {
                to_remove_by_etag
                    .entry(etag.clone())
                    .or_default()
                    .push(entry.etdr.ticket_id);
            }

            for (etag, ticket_ids) in to_remove_by_etag {
                if let Some(records) = data.get_mut(&etag) {
                    records.retain(|entry| !ticket_ids.contains(&entry.etdr.ticket_id));
                }
            }

            stats.evictions += to_remove as u64;

            // Remove empty entries
            data.retain(|_, records| !records.is_empty());
        }
    }

    /// Cleanup expired entries (only remove when already checkout and past 1h TTL)
    pub fn cleanup_expired(&self) -> usize {
        let mut data = self.data.lock().unwrap();
        let mut removed = 0;

        for records in data.values_mut() {
            let before = records.len();
            records.retain(|entry| etdr_within_ttl(&entry.etdr));
            removed += before - records.len();
        }

        // Remove empty entries
        data.retain(|_, records| !records.is_empty());

        removed
    }

    /// Remove entries not yet checkout with checkin older than max_age_ms (legacy).
    /// Returns number of entries removed.
    pub fn cleanup_no_checkout_older_than(&self, max_age_ms: i64) -> usize {
        let now = timestamp_ms();
        let mut data = self.data.lock().unwrap();
        let mut removed = 0;
        for records in data.values_mut() {
            let before = records.len();
            records.retain(|entry| {
                let no_checkout = entry.etdr.checkout_datetime == 0;
                let too_old = now.saturating_sub(entry.etdr.checkin_datetime) > max_age_ms;
                !(no_checkout && too_old)
            });
            removed += before - records.len();
        }
        data.retain(|_, records| !records.is_empty());
        removed
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> CacheStats {
        let data = self.data.lock().unwrap();
        let stats = self.stats.lock().unwrap();

        CacheStats {
            total_entries: data.values().map(|v| v.len()).sum(),
            total_etags: data.len(),
            hits: stats.hits,
            misses: stats.misses,
            evictions: stats.evictions,
        }
    }

    /// Get entry count in cache
    pub fn count(&self) -> usize {
        let data = self.data.lock().unwrap();
        data.values().map(|records| records.len()).sum()
    }

    /// Get etag count in cache
    pub fn etag_count(&self) -> usize {
        let data = self.data.lock().unwrap();
        data.len()
    }

    /// Get all ETDR with db_saved=false for DB save retry
    /// Returns: Vec<(etag, ETDR, created_at)>
    pub fn get_all_unsaved_etdrs(&self) -> Vec<(String, ETDR, i64)> {
        let data = self.data.lock().unwrap();
        let mut result = Vec::new();

        for (etag, records) in data.iter() {
            for entry in records.iter() {
                if etdr_within_ttl(&entry.etdr) && !entry.etdr.db_saved {
                    result.push((etag.clone(), entry.etdr.clone(), entry.created_at));
                }
            }
        }

        result
    }

    /// Remove all entries
    pub fn clear(&self) {
        let mut data = self.data.lock().unwrap();
        data.clear();

        let mut stats = self.stats.lock().unwrap();
        *stats = CacheStats::default();
    }

    /// Remove entries for a given etag (all transactions for that etag).
    pub fn remove_etag(&self, etag: &str) -> bool {
        let etag_key = normalize_etag(etag);
        let mut data = self.data.lock().unwrap();
        data.remove(&etag_key).is_some()
    }

    /// Remove exactly one entry by ticket_id (on committed checkout remove only that transaction, not whole etag).
    pub fn remove_by_ticket_id(&self, ticket_id: i64) -> bool {
        if ticket_id == 0 {
            return false;
        }
        let mut data = self.data.lock().unwrap();
        for records in data.values_mut() {
            let before = records.len();
            records.retain(|e| e.etdr.ticket_id != ticket_id);
            if records.len() != before {
                data.retain(|_, r| !r.is_empty());
                return true;
            }
        }
        false
    }
}

impl Default for ETDRCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Global ETDR cache instance (singleton).
/// Uses OnceLock so it is initialized only once.
static GLOBAL_ETDR_CACHE: std::sync::OnceLock<ETDRCache> = std::sync::OnceLock::new();

/// KeyDB used to sync latest ETDR to KeyDB (one key per etag, always overwrite with latest).
static KEYDB_FOR_ETDR_SYNC: std::sync::OnceLock<Arc<KeyDB>> = std::sync::OnceLock::new();

/// Get global ETDR cache instance
pub fn get_etdr_cache() -> &'static ETDRCache {
    GLOBAL_ETDR_CACHE.get_or_init(ETDRCache::new)
}

/// Register KeyDB so each ETDR save syncs latest to KeyDB (key = `etdr:{etag_id}`).
/// Call from main after creating CacheManager/KeyDB.
pub fn set_keydb_for_etdr_sync(keydb: Arc<KeyDB>) {
    let _ = KEYDB_FOR_ETDR_SYNC.set(keydb);
}

/// Get KeyDB used for sync (called from sync module).
pub(crate) fn get_keydb_for_etdr_sync() -> Option<Arc<KeyDB>> {
    KEYDB_FOR_ETDR_SYNC.get().cloned()
}

/// Lock key for ETDR retry (HA: only one node processes each KeyDB key per cycle).
fn etdr_retry_lock_key(etdr_key: &str) -> String {
    format!(
        "{}{}",
        crate::constants::etdr::ETDR_RETRY_LOCK_PREFIX,
        etdr_key
    )
}

/// Try to acquire retry lock for ETDR key (KeyDB SET NX EX). Returns true if acquired.
pub async fn try_acquire_etdr_retry_lock(etdr_key: &str) -> bool {
    let keydb = match KEYDB_FOR_ETDR_SYNC.get() {
        Some(k) => k.clone(),
        None => return true, // No KeyDB then no lock (single-node)
    };
    let lock_key = etdr_retry_lock_key(etdr_key);
    let value = std::env::var("HOSTNAME").unwrap_or_else(|_| "node".to_string());
    match keydb
        .set_nx_ex(
            &lock_key,
            &value,
            crate::constants::etdr::ETDR_RETRY_LOCK_TTL_SECS,
        )
        .await
    {
        Ok(acquired) => acquired,
        Err(e) => {
            tracing::warn!(
                etdr_key = %etdr_key,
                error = %e,
                "[Processor] ETDR retry lock acquire failed, skip key"
            );
            false
        }
    }
}

/// Release retry lock after processing done (success or failure).
pub async fn release_etdr_retry_lock(etdr_key: &str) {
    if let Some(keydb) = KEYDB_FOR_ETDR_SYNC.get() {
        let lock_key = etdr_retry_lock_key(etdr_key);
        let _ = keydb.remove(&lock_key).await;
    }
}

fn tcd_save_lock_key(transport_trans_id: i64) -> String {
    format!(
        "{}{}",
        crate::constants::etdr::TCD_SAVE_LOCK_PREFIX,
        transport_trans_id
    )
}

/// Try to acquire TCD save lock for transport_trans_id (KeyDB SET NX EX). Serializes save_tcd_list
/// only for the same transport_trans_id; different transport_trans_id use different keys so they
/// run concurrently (no global lock). Avoids duplicate (toll_a, toll_b) when checkin, sync and
/// retry run concurrently for the same transaction.
pub async fn try_acquire_tcd_save_lock(transport_trans_id: i64) -> bool {
    let keydb = match KEYDB_FOR_ETDR_SYNC.get() {
        Some(k) => k.clone(),
        None => return true, // No KeyDB then no lock (single-node)
    };
    let lock_key = tcd_save_lock_key(transport_trans_id);
    let value = std::env::var("HOSTNAME").unwrap_or_else(|_| "node".to_string());
    match keydb
        .set_nx_ex(
            &lock_key,
            &value,
            crate::constants::etdr::TCD_SAVE_LOCK_TTL_SECS,
        )
        .await
    {
        Ok(acquired) => acquired,
        Err(e) => {
            tracing::warn!(
                transport_trans_id,
                error = %e,
                "[Processor] TCD save lock acquire failed"
            );
            false
        }
    }
}

/// Release TCD save lock after save_tcd_list done.
pub async fn release_tcd_save_lock(transport_trans_id: i64) {
    if let Some(keydb) = KEYDB_FOR_ETDR_SYNC.get() {
        let lock_key = tcd_save_lock_key(transport_trans_id);
        let _ = keydb.remove(&lock_key).await;
    }
}

/// Save list of BECT TCD not yet saved to DB to KeyDB for later retry (key = pending_tcd:{transport_trans_id}).
pub async fn set_pending_tcd_bect(transport_trans_id: i64, list: &[TransportTransStageTcd]) {
    if list.is_empty() {
        return;
    }
    if let Some(keydb) = KEYDB_FOR_ETDR_SYNC.get() {
        let key = format!("{}{}", PENDING_TCD_KEY_PREFIX, transport_trans_id);
        let vec: Vec<TransportTransStageTcd> = list.to_vec();
        keydb.set(&key, &vec).await;
        tracing::info!(
            transport_trans_id,
            count = list.len(),
            "[Cache] BECT pending TCD saved to KeyDB for retry"
        );
    } else {
        tracing::warn!(
            transport_trans_id,
            count = list.len(),
            "[Cache] BECT pending TCD not saved: KeyDB not configured, retry will not persist"
        );
    }
}

/// Get pending BECT TCD list from KeyDB (returns None if absent or KeyDB not configured).
pub async fn get_pending_tcd_bect(transport_trans_id: i64) -> Option<Vec<TransportTransStageTcd>> {
    let keydb = KEYDB_FOR_ETDR_SYNC.get()?.clone();
    let key = format!("{}{}", PENDING_TCD_KEY_PREFIX, transport_trans_id);
    keydb.get::<Vec<TransportTransStageTcd>>(&key).await
}

/// Remove pending BECT TCD key after DB save succeeded.
pub async fn remove_pending_tcd_bect(transport_trans_id: i64) {
    if let Some(keydb) = KEYDB_FOR_ETDR_SYNC.get() {
        let key = format!("{}{}", PENDING_TCD_KEY_PREFIX, transport_trans_id);
        let _ = keydb.remove(&key).await;
        tracing::debug!(
            transport_trans_id,
            "[Cache] BECT pending TCD key removed from KeyDB"
        );
    }
}

/// Sync one ETDR record to KeyDB (key = etdr:{etag_id}). Called internally when updating hub_id in cache.
fn sync_etdr_to_keydb_if_set(etdr: &ETDR) {
    if let Some(keydb) = KEYDB_FOR_ETDR_SYNC.get() {
        let key = format!("etdr:{}", etdr.etag_id.trim());
        let keydb = keydb.clone();
        let etdr = etdr.clone();
        tokio::spawn(async move {
            keydb.set(&key, &etdr).await;
        });
    }
}

/// Save ETDR to global cache and sync to KeyDB.
/// ETDR/KeyDB: key etdr:{etag} only — unique per tag, overwritten on each checkin IN (and on every save_etdr) with latest ETDR.
/// Retries: key etdr_retry:{ticket_id}. When updating ETDR with !db_saved && ticket_id != 0, read existing from retries if any, merge so retries has latest unified data, then write back to retries by ticket_id.
pub fn save_etdr(etdr: ETDR) {
    use crate::models::ETDR::merge_etdr_same_ticket_id;
    get_etdr_cache().put(etdr.clone());
    if let Some(keydb) = KEYDB_FOR_ETDR_SYNC.get() {
        let key_etag = format!("etdr:{}", normalize_etag(&etdr.etag_id));
        let keydb = keydb.clone();
        let etdr_clone = etdr.clone();
        if !etdr.db_saved && etdr.ticket_id != 0 {
            let key_retry = format!("{}{}", ETDR_RETRY_KEY_PREFIX, etdr.ticket_id);
            tokio::spawn(async move {
                let existing = keydb.get::<ETDR>(&key_retry).await;
                let to_save_retry = match existing {
                    Some(ex) => merge_etdr_same_ticket_id(&ex, &etdr_clone),
                    None => etdr_clone.clone(),
                };
                keydb.set(&key_retry, &to_save_retry).await;
                keydb.set(&key_etag, &etdr_clone).await;
            });
        } else {
            tokio::spawn(async move {
                keydb.set(&key_etag, &etdr_clone).await;
            });
        }
    }
}

/// Update ETDR in cache memory only (no KeyDB write). Use when KeyDB already synced elsewhere or key will be removed.
pub fn save_etdr_to_memory_only(etdr: ETDR) {
    get_etdr_cache().put(etdr);
}

/// Remove ETDR from cache memory and KeyDB after transaction complete (after checkout commit).
/// When transport_trans_id present: remove only that ticket_id (memory + etdr_retry:{ticket_id}); do not remove etdr:etag so other transactions for same etag are not lost.
/// When transport_trans_id is None: remove whole etag from memory and remove key etdr:etag.
pub async fn clear_etdr_after_transaction_complete(etag: &str, transport_trans_id: Option<i64>) {
    let etag_key = normalize_etag(etag);
    if let Some(tid) = transport_trans_id {
        get_etdr_cache().remove_by_ticket_id(tid);
        remove_pending_tcd_bect(tid).await;
        if let Some(keydb) = KEYDB_FOR_ETDR_SYNC.get() {
            let key_retry = format!("{}{}", ETDR_RETRY_KEY_PREFIX, tid);
            let _ = keydb.remove(&key_retry).await;
        }
    } else {
        get_etdr_cache().remove_etag(&etag_key);
        if let Some(keydb) = KEYDB_FOR_ETDR_SYNC.get() {
            let key = format!("etdr:{}", etag_key);
            let _ = keydb.remove(&key).await;
        }
    }
}

/// Check ETDR still within TTL. TTL only applies when already checkout (1h from checkout); not checkout then no TTL expiry.
fn etdr_within_ttl(etdr: &ETDR) -> bool {
    if etdr.checkout_datetime == 0 {
        return true; // Not checkout: no TTL, treat as still valid
    }
    let now_ms = timestamp_ms();
    now_ms.saturating_sub(etdr.checkout_datetime) <= ETDR_TTL_MS
}

/// Get all ETDR from KeyDB for DB retry: etdr_retry:{ticket_id} (one per ticket_id, full retry data) and legacy etdr:{etag} with db_saved=false.
/// Caller should dedupe/merge by ticket_id so only one record per ticket_id is processed.
pub(crate) async fn get_all_etdrs_from_keydb_for_db_sync() -> Vec<(String, ETDR)> {
    let keydb = match KEYDB_FOR_ETDR_SYNC.get() {
        Some(k) => k.clone(),
        None => return Vec::new(),
    };
    let mut keys = match keydb.get_keys_with_prefix(ETDR_RETRY_KEY_PREFIX).await {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!(error = %e, "[Processor] ETDR retry load from KeyDB get_keys failed");
            return Vec::new();
        }
    };
    let legacy_keys = keydb
        .get_keys_with_prefix("etdr:")
        .await
        .unwrap_or_default();
    keys.extend(legacy_keys);
    if keys.is_empty() {
        return Vec::new();
    }
    let values: Vec<Option<ETDR>> = match keydb.get_many::<ETDR>(&keys).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "[Processor] ETDR load from KeyDB MGET failed");
            return Vec::new();
        }
    };
    let mut result = Vec::with_capacity(keys.len());
    for (key, opt) in keys.into_iter().zip(values) {
        if let Some(etdr) = opt {
            if !etdr.db_saved {
                result.push((key, etdr));
            }
        }
    }
    result
}

/// Flush all in-memory ETDRs with db_saved=false to KeyDB (etdr_retry:{ticket_id} and etdr:{etag})
/// so they can be retried after restart. Call on graceful shutdown.
pub async fn flush_unsaved_etdrs_to_keydb() -> u32 {
    use crate::models::ETDR::merge_etdr_same_ticket_id;
    use std::collections::HashMap;

    let unsaved = get_etdr_cache().get_all_unsaved_etdrs();
    if unsaved.is_empty() {
        return 0;
    }
    let keydb = match KEYDB_FOR_ETDR_SYNC.get() {
        Some(k) => k.clone(),
        None => {
            tracing::debug!(
                count = unsaved.len(),
                "[ETDR] shutdown: no KeyDB, skip flush unsaved ETDRs"
            );
            return 0;
        }
    };
    // Dedupe by ticket_id, merge when same ticket_id
    let mut by_ticket: HashMap<i64, (String, ETDR)> = HashMap::new();
    for (_etag, etdr, _) in unsaved {
        if etdr.ticket_id == 0 {
            continue;
        }
        let key_etag = format!("etdr:{}", normalize_etag(&etdr.etag_id));
        if let Some((_existing_etag, existing)) = by_ticket.get_mut(&etdr.ticket_id) {
            let merged = merge_etdr_same_ticket_id(existing, &etdr);
            *existing = merged;
        } else {
            by_ticket.insert(etdr.ticket_id, (key_etag, etdr));
        }
    }
    let mut flushed = 0u32;
    for (ticket_id, (_key_etag, etdr)) in by_ticket {
        let key_retry = format!("{}{}", ETDR_RETRY_KEY_PREFIX, ticket_id);
        let key_etag = format!("etdr:{}", normalize_etag(&etdr.etag_id));
        if keydb.set_result(&key_retry, &etdr).await.is_ok() {
            let _ = keydb.set(&key_etag, &etdr).await;
            flushed += 1;
        }
    }
    if flushed > 0 {
        tracing::info!(
            count = flushed,
            "[ETDR] shutdown: flushed unsaved ETDRs to KeyDB for retry"
        );
    }
    flushed
}

/// Get latest transaction by etag: cache only (by ticket_id descending). When ticket_id known use get_etdr_by_ticket_id for exact transaction.
/// KeyDB is no longer used for loading check-in; on cache miss caller should query DB/sync tables.
pub async fn get_latest_checkin_by_etag(etag: &str) -> Option<ETDR> {
    tracing::debug!(etag = %etag, "[Cache] Get latest checkin by etag");
    let start = std::time::Instant::now();
    if let Some(etdr) = get_etdr_cache().get_latest_checkin(etag) {
        let elapsed_ms = start.elapsed().as_millis() as u64;
        tracing::debug!(etag = %etag, elapsed_ms, ticket_id = etdr.ticket_id, station_id = etdr.station_id, lane_id = etdr.lane_id, "[Cache] get_latest_checkin_by_etag cache hit");
        return Some(etdr);
    }

    tracing::debug!(etag = %etag, "[Cache] Get latest checkin by etag miss");
    None
}

/// Checkin data source: route to correct repository on cache miss.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EtdrCheckinSource {
    /// BECT: query TRANSPORT_TRANSACTION_STAGE only
    Bect,
    /// VETC/VDTC: query BOO_TRANSPORT_TRANS_STAGE only
    #[default]
    VetcVdtc,
    /// Both tables, pick record with latest checkin_datetime (fallback when type unknown)
    Any,
}

/// Get latest checkin ETDR for etag without checkout info (toll_out == 0).
/// Only looks in cache memory. On cache miss handler calls get_best_pending_from_sync_or_main_* to search sync + main table for closer record.
pub async fn get_latest_checkin_without_checkout(
    etag: &str,
    _source: EtdrCheckinSource,
) -> Option<ETDR> {
    if let Some(etdr) = get_etdr_cache().get_latest_checkin_without_checkout(etag) {
        return Some(etdr);
    }
    None
}

/// Update ETDR atomically in checkout (one etag one thread at a time). Returns Some(updated ETDR) or None.
pub async fn atomic_update_etdr_for_checkout<F>(
    etag: &str,
    ref_trans_id: i64,
    updater: F,
) -> Option<ETDR>
where
    F: FnOnce(&mut ETDR) + Send + 'static,
{
    let etag_key = normalize_etag(etag);

    // Use spawn_blocking to run blocking operation in async context
    tokio::task::spawn_blocking(move || {
        let cache = get_etdr_cache();
        let etag_lock = cache.get_etag_lock(&etag_key);
        let _write_guard = etag_lock.write().unwrap();

        // Get current ETDR
        let mut data = cache.data.lock().unwrap();
        if let Some(records) = data.get_mut(&etag_key) {
            for entry in records.iter_mut() {
                if entry.etdr.ref_trans_id == ref_trans_id && entry.etdr.toll_out == 0 {
                    // Clone ETDR to update
                    let mut etdr = entry.etdr.clone();

                    // Apply updater
                    updater(&mut etdr);

                    // Update entry
                    entry.etdr = etdr.clone();
                    entry.last_access.store(timestamp_ms(), Ordering::Relaxed);

                    return Some(etdr);
                }
            }
        }
        None
    })
    .await
    .unwrap_or(None)
}

/// Get ETDR by ticket_id from global cache
pub fn get_etdr_by_ticket_id(ticket_id: i64) -> Option<ETDR> {
    get_etdr_cache().get_by_ticket_id(ticket_id)
}

/// Remove ETDR from KeyDB if not checkout and checkin older than max_age_ms. Uses batch DEL and sync memory removal.
async fn cleanup_keydb_no_checkout_older_than(max_age_ms: i64) -> usize {
    let keydb = match KEYDB_FOR_ETDR_SYNC.get() {
        Some(k) => k.clone(),
        None => return 0,
    };
    let keys = match keydb.get_keys_with_prefix("etdr:").await {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!(error = %e, "[Cache] ETDR cleanup KeyDB get_keys failed");
            return 0;
        }
    };
    if keys.is_empty() {
        return 0;
    }
    let values: Vec<Option<ETDR>> = match keydb.get_many::<ETDR>(&keys).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "[Cache] ETDR cleanup KeyDB MGET failed");
            return 0;
        }
    };
    let now = timestamp_ms();
    let to_remove: Vec<(String, ETDR)> = keys
        .into_iter()
        .zip(values)
        .filter_map(|(key, opt)| {
            let etdr = opt?;
            let no_checkout = etdr.checkout_datetime == 0;
            let too_old = now.saturating_sub(etdr.checkin_datetime) > max_age_ms;
            if no_checkout && too_old {
                Some((key, etdr))
            } else {
                None
            }
        })
        .collect();
    let n = to_remove.len();
    if n == 0 {
        return 0;
    }
    let keys_to_del: Vec<String> = to_remove.iter().map(|(k, _)| k.clone()).collect();
    if let Err(e) = keydb.remove_many(&keys_to_del).await {
        tracing::warn!(error = %e, "[Cache] ETDR cleanup KeyDB batch DEL failed");
        return 0;
    }
    let cache = get_etdr_cache();
    for (_, etdr) in to_remove {
        cache.remove_etag(etdr.etag_id.trim());
    }
    n
}

/// Start background task: cleanup expired (checkout past TTL), then remove no-checkout older than 3 days (KeyDB first, then memory).
pub fn start_cache_cleanup_task(interval_secs: u64) {
    let cache = get_etdr_cache().clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            // 1. Checkout past 1h TTL
            let removed = cache.cleanup_expired();
            if removed > 0 {
                tracing::debug!(removed, "[Cache] ETDR cleaned expired");
            }
            // 2. No checkout >3 days: KeyDB first (batch DEL + remove_etag) to reduce memory work
            let removed_keydb = cleanup_keydb_no_checkout_older_than(NO_CHECKOUT_MAX_AGE_MS).await;
            if removed_keydb > 0 {
                tracing::info!(
                    removed = removed_keydb,
                    "[Cache] ETDR cleaned no-checkout >3d (KeyDB)"
                );
            }
            // 3. Memory: remove old no-checkout entries (etags not in KeyDB or still have other entries)
            let removed_3d = cache.cleanup_no_checkout_older_than(NO_CHECKOUT_MAX_AGE_MS);
            if removed_3d > 0 {
                tracing::info!(
                    removed = removed_3d,
                    "[Cache] ETDR cleaned no-checkout >3d (memory)"
                );
            }
        }
    });
}
