//! TCD save with lock per transport_trans_id to avoid duplicate rows (toll_a, toll_b) when
//! checkin, sync-from-DB and retry task run concurrently for the **same** transaction.
//!
//! Lock key is `lock:tcd_save:{transport_trans_id}`: only the same transport_trans_id is
//! serialized; different transport_trans_id use different keys so saves run **concurrently**.
//!
//! All callers that write TCD for a given transport_trans_id must use these helpers so only one
//! save_tcd_list runs at a time per transport_trans_id; save_tcd_list itself already merges by
//! (toll_a_id, toll_b_id) but without a lock two concurrent calls can both see existing=[] and
//! both insert the same (toll_a, toll_b).

use crate::db::repositories::TransportTransStageTcd;
use crate::models::ETDR::{release_tcd_save_lock, try_acquire_tcd_save_lock};
use crate::services::service::ServiceError;
use crate::services::TransportTransStageTcdService;

/// Save BECT TCD list with lock. Returns Err if lock not acquired (caller should set_pending_tcd_bect).
pub async fn save_bect_tcd_list_with_lock(
    transport_trans_id: i64,
    to_save: &[TransportTransStageTcd],
) -> Result<(), ServiceError> {
    if !try_acquire_tcd_save_lock(transport_trans_id).await {
        return Err(ServiceError::Other(
            "TCD save lock not acquired (concurrent save), use pending retry".to_string(),
        ));
    }
    let result = TransportTransStageTcdService::new()
        .save_tcd_list(transport_trans_id, to_save)
        .await;
    release_tcd_save_lock(transport_trans_id).await;
    result
}
