//! Cache segments from RATING_OWNER.CLOSED_CYCLE_TRANSITION_STAGE: key (TOLL_A, TOLL_B) -> Vec<CLOSED_CYCLE_TRANSITION_ID>.
//! Load at startup like Price; cache-aside when segment requested and missing (DB then cache).
//! Cycle_id for segment comes from TOLL_STAGE by stage_id (loaded once when loading CLOSED_CYCLE_TRANSITION_STAGE).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use odbc_api::Cursor;
use once_cell::sync::Lazy;
use r2d2::Pool;
use tokio::task;

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::config::cache_prefix::CachePrefix;
use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::{format_sql_value, get_i64, get_nullable_i64, DbError};

/// In-memory: (lo, hi) -> Vec<closed_cycle_transition_id>. Key normalized so (a, b) and (b, a) share same key.
static SEGMENT_MEMORY: Lazy<Mutex<HashMap<(i64, i64), Vec<i64>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
/// closed_cycle_transition_id -> cycle_id from TOLL_STAGE, filled when loading CLOSED_CYCLE_TRANSITION_STAGE.
static CYCLE_BY_STAGE: Lazy<Mutex<HashMap<i64, i64>>> = Lazy::new(|| Mutex::new(HashMap::new()));
/// stage_id (toll_stage_id) -> (TOLL_A, TOLL_B) from TOLL_STAGE; used to fill ratingDetail.toll_a_id/toll_b_id from toll_stage.
static TOLL_AB_BY_STAGE: Lazy<Mutex<HashMap<i64, (i64, i64)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn segment_key(a: i64, b: i64) -> (i64, i64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn replace_all_segments(data: Vec<((i64, i64), Vec<i64>)>) {
    let map = data.into_iter().collect::<HashMap<_, _>>();
    if let Ok(mut guard) = SEGMENT_MEMORY.lock() {
        *guard = map;
    }
}

fn set_cycle_by_stage(map: HashMap<i64, i64>) {
    if let Ok(mut guard) = CYCLE_BY_STAGE.lock() {
        *guard = map;
    }
}

fn set_toll_ab_by_stage(map: HashMap<i64, (i64, i64)>) {
    if let Ok(mut guard) = TOLL_AB_BY_STAGE.lock() {
        *guard = map;
    }
}

fn get_segments_from_memory(toll_a: i64, toll_b: i64) -> Option<Vec<i64>> {
    let key = segment_key(toll_a, toll_b);
    SEGMENT_MEMORY
        .lock()
        .ok()
        .and_then(|g| g.get(&key).cloned())
}

fn set_segment_memory(toll_a: i64, toll_b: i64, transition_ids: Vec<i64>) {
    let key = segment_key(toll_a, toll_b);
    if let Ok(mut guard) = SEGMENT_MEMORY.lock() {
        guard.insert(key, transition_ids);
    }
}

/// Returns cycle_id for segment (toll_a, toll_b) using first closed_cycle_transition_id from CLOSED_CYCLE_TRANSITION_STAGE and TOLL_STAGE. Sync, memory only.
#[inline]
pub fn get_segment_cycle_id(toll_a: i64, toll_b: i64) -> Option<i64> {
    let stage_id = get_stage_id_for_segment(toll_a, toll_b)?;
    CYCLE_BY_STAGE
        .lock()
        .ok()
        .and_then(|g| g.get(&stage_id).copied())
}

/// Returns first closed_cycle_transition_id (as stage_id for pricing) for segment (toll_a, toll_b) from cache. Sync, memory only.
#[inline]
pub fn get_stage_id_for_segment(toll_a: i64, toll_b: i64) -> Option<i64> {
    get_segments_from_memory(toll_a, toll_b).and_then(|v| v.into_iter().next())
}

/// Returns (TOLL_A, TOLL_B) from TOLL_STAGE for the given stage_id (cycle_id = toll_stage_id). Use these for ratingDetail.toll_a_id/toll_b_id.
#[inline]
pub fn get_toll_a_toll_b_for_stage_id(stage_id: i64) -> Option<(i64, i64)> {
    TOLL_AB_BY_STAGE
        .lock()
        .ok()
        .and_then(|g| g.get(&stage_id).copied())
}

/// Returns all transition ids for segment (toll_a, toll_b). Sync, memory only.
#[allow(dead_code)]
pub fn get_segments_from_memory_sync(toll_a: i64, toll_b: i64) -> Option<Vec<i64>> {
    get_segments_from_memory(toll_a, toll_b)
}

/// Load cache from RATING_OWNER.CLOSED_CYCLE_TRANSITION_STAGE. Prefer DB; on failure fallback KeyDB when `load_from_keydb` true.
/// Also loads CLOSED_CYCLE_TRANSITION_ID -> CYCLE_ID from TOLL_STAGE for segments present in CLOSED_CYCLE_TRANSITION_STAGE.
pub async fn get_closed_cycle_transition_stage_cache(
    pool_opt: Option<Arc<Pool<OdbcConnectionManager>>>,
    cache: Arc<CacheManager>,
    load_from_keydb: bool,
) {
    let prefix = CachePrefix::ClosedCycleTransitionStage.as_str();
    let result = match pool_opt {
        Some(pool) => load_closed_cycle_transition_stage_from_db(pool).await,
        None => {
            tracing::warn!(
                "[Cache] ClosedCycleTransitionStage skip DB (pool not available), will use KeyDB fallback if enabled"
            );
            Err(DbError::ExecutionError("DB pool not available".to_string()))
        }
    };
    match result {
        Ok((segment_map, cycle_by_stage, toll_ab_by_stage)) => {
            let cache_data: Vec<(String, Vec<i64>)> = segment_map
                .iter()
                .map(|(&(lo, hi), ids)| {
                    let key = cache.gen_key(
                        CachePrefix::ClosedCycleTransitionStage,
                        &[&lo as &dyn std::fmt::Display, &hi as &dyn std::fmt::Display],
                    );
                    (key, ids.clone())
                })
                .collect();
            let n = cache_data.len();
            cache.atomic_reload_prefix(prefix, cache_data).await;
            replace_all_segments(segment_map.into_iter().collect::<Vec<_>>());
            set_cycle_by_stage(cycle_by_stage);
            set_toll_ab_by_stage(toll_ab_by_stage);
            if load_from_keydb {
                tracing::info!(
                    segments = n,
                    "[Cache] ClosedCycleTransitionStage loaded from DB, synced to KeyDB"
                );
            } else {
                tracing::info!(
                    segments = n,
                    "[Cache] ClosedCycleTransitionStage loaded from DB"
                );
            }
        }
        Err(e) => {
            tracing::error!(error = ?e, "[Cache] ClosedCycleTransitionStage load from DB failed");
            if !load_from_keydb {
                return;
            }
            let keydb_loaded = cache.load_prefix_from_keydb::<Vec<i64>>(prefix).await;
            if keydb_loaded.is_empty() {
                tracing::warn!(
                    "[Cache] ClosedCycleTransitionStage fallback KeyDB empty, keeping empty"
                );
                return;
            }
            let segment_map: HashMap<(i64, i64), Vec<i64>> = keydb_loaded
                .into_iter()
                .filter_map(|(k, v)| {
                    let parts: Vec<&str> = k.splitn(3, ':').collect();
                    if parts.len() >= 3 {
                        let lo = parts[1].parse::<i64>().ok()?;
                        let hi = parts[2].parse::<i64>().ok()?;
                        Some(((lo, hi), v))
                    } else {
                        None
                    }
                })
                .collect();
            let n = segment_map.len();
            replace_all_segments(segment_map.into_iter().collect());
            tracing::info!(
                segments = n,
                "[Cache] ClosedCycleTransitionStage loaded from KeyDB (DB failed, fallback)"
            );
        }
    }
}

async fn load_closed_cycle_transition_stage_from_db(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
) -> Result<
    (
        HashMap<(i64, i64), Vec<i64>>,
        HashMap<i64, i64>,
        HashMap<i64, (i64, i64)>,
    ),
    DbError,
> {
    task::spawn_blocking(move || {
        let query = r#"SELECT TOLL_A, TOLL_B, CLOSED_CYCLE_TRANSITION_ID FROM RATING_OWNER.CLOSED_CYCLE_TRANSITION_STAGE"#;
        let conn = get_connection_with_retry(db_pool.as_ref())?;
        let cursor_result = conn.execute(query, (), None)?;
        let mut segment_map: HashMap<(i64, i64), Vec<i64>> = HashMap::new();
        if let Some(mut cursor) = cursor_result {
            loop {
                match cursor.next_row() {
                    Ok(Some(mut row)) => {
                        let toll_a = get_i64(&mut row, 1)?;
                        let toll_b = get_i64(&mut row, 2)?;
                        let transition_id = get_i64(&mut row, 3)?;
                        let key = segment_key(toll_a, toll_b);
                        segment_map
                            .entry(key)
                            .or_default()
                            .push(transition_id);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return Err(DbError::ExecutionError(format!(
                            "Error fetching CLOSED_CYCLE_TRANSITION_STAGE row: {}",
                            e
                        )));
                    }
                }
            }
        }
        let stage_ids: Vec<i64> = segment_map
            .values()
            .flat_map(|v| v.iter().copied())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let cycle_by_stage = load_cycle_by_stage(&conn, &stage_ids)?;
        let toll_ab_by_stage = load_toll_ab_by_stage(&conn, &stage_ids)?;
        Ok((segment_map, cycle_by_stage, toll_ab_by_stage))
    })
    .await
    .map_err(|e| {
        DbError::ExecutionError(format!(
            "[ClosedCycleTransitionStageCache] load_closed_cycle_transition_stage_from_db spawn_blocking: {}",
            e
        ))
    })?
}

fn load_cycle_by_stage(
    conn: &odbc_api::Connection<'_>,
    stage_ids: &[i64],
) -> Result<HashMap<i64, i64>, DbError> {
    if stage_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let ids_str = stage_ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let query = format!(
        r#"SELECT STAGE_ID, CYCLE_ID FROM RATING_OWNER.TOLL_STAGE WHERE STAGE_ID IN ({})"#,
        ids_str
    );
    let cursor_result = conn.execute(&query, (), None)?;
    let mut map = HashMap::new();
    if let Some(mut cursor) = cursor_result {
        loop {
            match cursor.next_row() {
                Ok(Some(mut row)) => {
                    let stage_id = get_i64(&mut row, 1)?;
                    let cycle_id = get_i64(&mut row, 2)?;
                    map.insert(stage_id, cycle_id);
                }
                Ok(None) => break,
                Err(e) => {
                    return Err(DbError::ExecutionError(format!(
                        "Error fetching TOLL_STAGE cycle: {}",
                        e
                    )));
                }
            }
        }
    }
    Ok(map)
}

/// Load STAGE_ID -> (TOLL_A, TOLL_B) from TOLL_STAGE for given stage_ids. Only inserts when both TOLL_A and TOLL_B are non-null.
fn load_toll_ab_by_stage(
    conn: &odbc_api::Connection<'_>,
    stage_ids: &[i64],
) -> Result<HashMap<i64, (i64, i64)>, DbError> {
    if stage_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let ids_str = stage_ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let query = format!(
        r#"SELECT STAGE_ID, TOLL_A, TOLL_B FROM RATING_OWNER.TOLL_STAGE WHERE STAGE_ID IN ({})"#,
        ids_str
    );
    let cursor_result = conn.execute(&query, (), None)?;
    let mut map = HashMap::new();
    if let Some(mut cursor) = cursor_result {
        loop {
            match cursor.next_row() {
                Ok(Some(mut row)) => {
                    let stage_id = get_i64(&mut row, 1)?;
                    let toll_a = get_nullable_i64(&mut row, 2)?;
                    let toll_b = get_nullable_i64(&mut row, 3)?;
                    if let (Some(a), Some(b)) = (toll_a, toll_b) {
                        map.insert(stage_id, (a, b));
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    return Err(DbError::ExecutionError(format!(
                        "Error fetching TOLL_STAGE toll_a/toll_b: {}",
                        e
                    )));
                }
            }
        }
    }
    Ok(map)
}

/// Cache-aside: get segment transition ids for (toll_a, toll_b). Try (a,b) in memory, then (b,a) reverse direction; only if both miss then load from DB (single query for both directions).
pub async fn get_segments_for_leg(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
    toll_a: i64,
    toll_b: i64,
) -> Option<Vec<i64>> {
    if let Some(seg) = get_segments_from_memory(toll_a, toll_b) {
        tracing::debug!(
            toll_a,
            toll_b,
            "[Cache] ClosedCycleTransitionStage in-memory hit"
        );
        return Some(seg);
    }

    if let Some(seg) = get_segments_from_memory(toll_b, toll_a) {
        tracing::debug!(
            toll_b,
            toll_a,
            "[Cache] ClosedCycleTransitionStage in-memory hit (reverse)"
        );
        return Some(seg);
    }
    tracing::debug!(
        toll_a,
        toll_b,
        "[Cache] ClosedCycleTransitionStage in-memory miss, fallback to DB"
    );
    match load_segment_from_db(db_pool.clone(), toll_a, toll_b).await {
        Ok(Some(seg)) => {
            let (lo, hi) = segment_key(toll_a, toll_b);
            let key = cache.gen_key(
                CachePrefix::ClosedCycleTransitionStage,
                &[&lo as &dyn std::fmt::Display, &hi as &dyn std::fmt::Display],
            );
            cache.set(&key, &seg).await;
            set_segment_memory(toll_a, toll_b, seg.clone());
            tracing::debug!(
                toll_a,
                toll_b,
                "[Cache] ClosedCycleTransitionStage loaded from DB"
            );
            Some(seg)
        }
        Ok(None) => None,
        Err(e) => {
            tracing::error!(toll_a, toll_b, error = ?e, "[Cache] ClosedCycleTransitionStage DB query failed");
            None
        }
    }
}

async fn load_segment_from_db(
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    toll_a: i64,
    toll_b: i64,
) -> Result<Option<Vec<i64>>, DbError> {
    task::spawn_blocking(move || {
        let query = format!(
            r#"SELECT CLOSED_CYCLE_TRANSITION_ID FROM RATING_OWNER.CLOSED_CYCLE_TRANSITION_STAGE WHERE (TOLL_A = {} AND TOLL_B = {}) OR (TOLL_A = {} AND TOLL_B = {})"#,
            format_sql_value(&toll_a),
            format_sql_value(&toll_b),
            format_sql_value(&toll_b),
            format_sql_value(&toll_a)
        );
        let conn = get_connection_with_retry(db_pool.as_ref())?;
        let cursor_result = conn.execute(&query, (), None)?;
        let mut transition_ids = Vec::new();
        if let Some(mut cursor) = cursor_result {
            loop {
                match cursor.next_row() {
                    Ok(Some(mut row)) => {
                        let id = get_i64(&mut row, 1)?;
                        transition_ids.push(id);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return Err(DbError::ExecutionError(format!(
                            "[ClosedCycleTransitionStageCache] Error fetching segment: {}",
                            e
                        )));
                    }
                }
            }
        }
        Ok(if transition_ids.is_empty() {
            None
        } else {
            Some(transition_ids)
        })
    })
    .await
    .map_err(|e| {
        DbError::ExecutionError(format!(
            "[ClosedCycleTransitionStageCache] load_segment_from_db spawn_blocking: {}",
            e
        ))
    })?
}
