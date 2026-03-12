//! Background task: load ETDR not yet saved to DB from KeyDB and memory, dedupe/merge by ticket_id.
//! One record per cycle: lock by ticket_id (HA, SET NX), retry save to DB, then update KeyDB/memory with db_saved before releasing lock.
//! Retry store key etdr_retry:{ticket_id} is removed on success so only one record per ticket_id persists.

use crate::constants::etdr::{
    ETDR_MAX_DB_SAVE_RETRIES, ETDR_RETRY_BACKOFF_MS, ETDR_RETRY_KEY_PREFIX,
};
use crate::models::ETDR::{
    get_etdr_cache, release_etdr_retry_lock, try_acquire_etdr_retry_lock, ETDR,
};
use crate::utils::normalize_etag;
use std::collections::HashMap;
use std::time::Duration;

use super::retry::retry_save_etdr_to_db;

/// Merge unsaved ETDRs from KeyDB and from memory (both sources; dedupe_by_ticket_id afterwards).
fn collect_unsaved_etdrs(keydb_list: Vec<(String, ETDR)>) -> Vec<(String, ETDR)> {
    let mut out = keydb_list;
    for (_etag, etdr, _) in get_etdr_cache().get_all_unsaved_etdrs() {
        out.push((format!("etdr:{}", etdr.etag_id.trim()), etdr));
    }
    out
}

/// Keep one ETDR per ticket_id; when two records share the same ticket_id, merge them so retry store has a single merged record per ticket_id.
fn dedupe_by_ticket_id(to_save: Vec<(String, ETDR)>) -> Vec<(String, ETDR)> {
    use super::merge::merge_etdr_same_ticket_id;
    let mut by_ticket: HashMap<i64, (String, ETDR)> = HashMap::new();
    for (key, etdr) in to_save {
        if etdr.ticket_id == 0 {
            continue;
        }
        if let Some((existing_key, existing)) = by_ticket.get_mut(&etdr.ticket_id) {
            let merged = merge_etdr_same_ticket_id(existing, &etdr);
            let key_retry = format!("{}{}", ETDR_RETRY_KEY_PREFIX, etdr.ticket_id);
            let key_to_keep = if key == key_retry || *existing_key == key_retry {
                key_retry
            } else if etdr.checkin_datetime >= existing.checkin_datetime {
                key
            } else {
                std::mem::take(existing_key)
            };
            *existing_key = key_to_keep;
            *existing = merged;
        } else {
            by_ticket.insert(etdr.ticket_id, (key, etdr));
        }
    }
    by_ticket.into_values().collect()
}

/// Starts the background task: each cycle loads unsaved list, processes one record (lock, save DB, mark db_saved).
/// Only waits for interval when there are no pending records, so time to next record is minimal when backlog exists.
pub fn start_etdr_db_retry_task(interval_secs: u64, _max_retry_count: i32) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
        loop {
            let etdrs_from_keydb =
                crate::models::ETDR::get_all_etdrs_from_keydb_for_db_sync().await;

            let mut to_save = dedupe_by_ticket_id(collect_unsaved_etdrs(etdrs_from_keydb));

            if to_save.is_empty() {
                interval.tick().await;
                continue;
            }

            // One record per iteration: try candidates until lock acquired, then process.
            let mut processed = false;
            while let Some((key, etdr)) = to_save.pop() {
                let lock_key = etdr.ticket_id.to_string();
                if !try_acquire_etdr_retry_lock(&lock_key).await {
                    tracing::debug!(
                        ticket_id = etdr.ticket_id,
                        etag = %etdr.etag_number,
                        remaining = to_save.len(),
                        "[Processor] ETDR retry lock not acquired, try next record"
                    );
                    continue;
                }

                let success = retry_save_etdr_to_db(&etdr).await;
                let raw_content =
                    serde_json::to_string(&etdr).unwrap_or_else(|_| format!("{:?}", etdr));
                crate::logging::log_db_retry(&raw_content, success);
                let keydb = crate::models::ETDR::get_keydb_for_etdr_sync();

                if success {
                    let mut etdr_done = etdr;
                    etdr_done.db_saved = true;
                    let ticket_id = etdr_done.ticket_id;
                    let etag_number = etdr_done.etag_number.clone();
                    let committed_checkout = etdr_done.checkout_commit_datetime > 0;
                    let key_retry = format!("{}{}", ETDR_RETRY_KEY_PREFIX, ticket_id);
                    if let Some(ref k) = keydb {
                        let _ = k.remove(&key_retry).await;
                    }
                    if committed_checkout {
                        get_etdr_cache().remove_by_ticket_id(ticket_id);
                        if let Some(ref k) = keydb {
                            let _ = k.remove(&key).await;
                        }
                    } else {
                        let key_etag = format!("etdr:{}", normalize_etag(&etdr_done.etag_id));
                        if let Some(ref k) = keydb {
                            if let Err(e) = k.set_result(&key_etag, &etdr_done).await {
                                tracing::warn!(
                                    ticket_id,
                                    key = %key_etag,
                                    error = %e,
                                    "[Processor] ETDR retry KeyDB set db_saved failed"
                                );
                            }
                        }
                        crate::models::ETDR::save_etdr_to_memory_only(etdr_done);
                    }
                    // Release lock only after KeyDB/memory updated so other nodes won't load this record as unsaved next cycle.
                    release_etdr_retry_lock(&lock_key).await;
                    tracing::info!(
                        ticket_id,
                        etag = %etag_number,
                        pending = to_save.len(),
                        "[Processor] ETDR retry saved to DB, marked db_saved (no further sync)"
                    );
                } else {
                    let new_count = etdr.db_save_retry_count + 1;
                    let ticket_id = etdr.ticket_id;
                    let key_retry = format!("{}{}", ETDR_RETRY_KEY_PREFIX, ticket_id);

                    if new_count >= ETDR_MAX_DB_SAVE_RETRIES {
                        if let Some(ref k) = keydb {
                            let _ = k.remove(&key_retry).await;
                        }
                        get_etdr_cache().remove_by_ticket_id(ticket_id);
                        release_etdr_retry_lock(&lock_key).await;
                        tracing::warn!(
                            ticket_id,
                            etag = %etdr.etag_number,
                            retry_count = new_count,
                            max_retries = ETDR_MAX_DB_SAVE_RETRIES,
                            "[Processor] ETDR retry removed from list after max DB save failures"
                        );
                    } else {
                        let mut etdr_updated = etdr.clone();
                        etdr_updated.db_save_retry_count = new_count;
                        if let Some(ref k) = keydb {
                            if let Err(e) = k.set_result(&key_retry, &etdr_updated).await {
                                tracing::warn!(
                                    ticket_id,
                                    key = %key_retry,
                                    error = %e,
                                    "[Processor] ETDR retry KeyDB update retry count failed"
                                );
                            }
                        }
                        release_etdr_retry_lock(&lock_key).await;
                        tracing::warn!(
                            ticket_id = etdr.ticket_id,
                            etag = %etdr.etag_number,
                            retry_count = new_count,
                            "[Processor] ETDR retry save to DB failed, will retry next cycle"
                        );
                    }
                }
                processed = true;
                break;
            }

            if processed {
                tokio::time::sleep(Duration::from_millis(ETDR_RETRY_BACKOFF_MS)).await;
            }
            if !processed {
                tracing::debug!(
                    "[Processor] ETDR retry no record processed this cycle (all locked or empty)"
                );
                interval.tick().await;
            }
        }
    });
}
