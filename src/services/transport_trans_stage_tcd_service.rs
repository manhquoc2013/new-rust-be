//! Service TRANSPORT_TRANS_STAGE_TCD (insert, find by transport_trans_id).

use crate::db::repositories::{TransportTransStageTcd, TransportTransStageTcdRepository};
use crate::db::Repository;
use crate::services::service::ServiceError;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Key (toll_a_id, toll_b_id) for matching TCD. Uses 0 when None.
fn tcd_key(toll_a_id: Option<i64>, toll_b_id: Option<i64>) -> (i64, i64) {
    (toll_a_id.unwrap_or(0), toll_b_id.unwrap_or(0))
}

/// Service TRANSPORT_TRANS_STAGE_TCD (TCD detail table, no complex cache).
pub struct TransportTransStageTcdService {
    repository: Arc<TransportTransStageTcdRepository>,
}

impl TransportTransStageTcdService {
    pub fn new() -> Self {
        Self {
            repository: Arc::new(TransportTransStageTcdRepository::new()),
        }
    }

    /// Get TCD list by TRANSPORT_TRANS_ID.
    pub async fn find_by_transport_trans_id(
        &self,
        transport_trans_id: i64,
    ) -> Result<Vec<TransportTransStageTcd>, ServiceError> {
        let repo = self.repository.clone();
        match tokio::task::spawn_blocking(move || {
            repo.find_by_transport_trans_id(transport_trans_id)
        })
        .await
        {
            Ok(Ok(list)) => Ok(list),
            Ok(Err(e)) => Err(ServiceError::DatabaseError(e)),
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Delete all TCD records by TRANSPORT_TRANS_ID (clear before re-saving).
    pub async fn delete_by_transport_trans_id(
        &self,
        transport_trans_id: i64,
    ) -> Result<(), ServiceError> {
        let repo = self.repository.clone();
        match tokio::task::spawn_blocking(move || {
            repo.delete_by_transport_trans_id(transport_trans_id)
        })
        .await
        {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(ServiceError::DatabaseError(e)),
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Delete one TCD record by primary key.
    pub async fn delete_by_id(&self, id: i64) -> Result<(), ServiceError> {
        let repo = self.repository.clone();
        match tokio::task::spawn_blocking(move || repo.delete(id)).await {
            Ok(Ok(true)) | Ok(Ok(false)) => Ok(()),
            Ok(Err(e)) => Err(ServiceError::DatabaseError(e)),
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Update TCD record (entity must have existing transport_stage_tcd_id in DB).
    pub async fn update(&self, entity: &TransportTransStageTcd) -> Result<(), ServiceError> {
        let repo = self.repository.clone();
        let id = entity.transport_stage_tcd_id;
        let entity_clone = entity.clone();
        match tokio::task::spawn_blocking(move || repo.update(id, &entity_clone)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(ServiceError::DatabaseError(e)),
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Check whether any TCD record exists for TRANSPORT_TRANS_ID in DB.
    pub async fn has_any_by_transport_trans_id(
        &self,
        transport_trans_id: i64,
    ) -> Result<bool, ServiceError> {
        let repo = self.repository.clone();
        match tokio::task::spawn_blocking(move || {
            repo.find_by_transport_trans_id(transport_trans_id)
                .map(|v| !v.is_empty())
        })
        .await
        {
            Ok(Ok(has)) => Ok(has),
            Ok(Err(e)) => Err(ServiceError::DatabaseError(e)),
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Insert TransportTransStageTcd record.
    pub async fn insert(&self, entity: &TransportTransStageTcd) -> Result<i64, ServiceError> {
        let repo = self.repository.clone();
        let entity_clone = entity.clone();

        match tokio::task::spawn_blocking(move || repo.insert(&entity_clone)).await {
            Ok(result) => match result {
                Ok(tcd_id) => Ok(tcd_id),
                Err(e) => Err(ServiceError::DatabaseError(e)),
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Sync TCD list with DB by (toll_a_id, toll_b_id):
    /// - Load existing TCD by transport_trans_id.
    /// - If (toll_a, toll_b) to save already exists → update.
    /// - If (toll_a, toll_b) to save is new → insert.
    /// - If existing TCD (toll_a, toll_b) is not in to_save → delete.
    /// Order: update/insert first, then delete (avoid updating a deleted row). When to_save is empty, call delete_by_transport_trans_id once.
    /// Duplicate (toll_a, toll_b) in to_save are deduped (last per key). Callers must use save_bect_tcd_list_with_lock (ETDR::tcd_save) to serialize per transport_trans_id and avoid duplicate rows when checkin/sync/retry run concurrently.
    pub async fn save_tcd_list(
        &self,
        transport_trans_id: i64,
        to_save: &[TransportTransStageTcd],
    ) -> Result<(), ServiceError> {
        if to_save.is_empty() {
            return self.delete_by_transport_trans_id(transport_trans_id).await;
        }

        let deduped: Vec<TransportTransStageTcd> = to_save
            .iter()
            .fold(
                HashMap::<(i64, i64), TransportTransStageTcd>::new(),
                |mut m, t| {
                    m.insert(tcd_key(t.toll_a_id, t.toll_b_id), t.clone());
                    m
                },
            )
            .into_values()
            .collect();

        let existing = self.find_by_transport_trans_id(transport_trans_id).await?;
        let existing_by_key: HashMap<(i64, i64), TransportTransStageTcd> = existing
            .into_iter()
            .map(|e| (tcd_key(e.toll_a_id, e.toll_b_id), e))
            .collect();
        let keys_to_save: HashSet<(i64, i64)> = deduped
            .iter()
            .map(|t| tcd_key(t.toll_a_id, t.toll_b_id))
            .collect();

        // Cập nhật hoặc thêm mới từng TCD cần lưu (trước khi xóa, tránh update vào bản ghi đã xóa).
        for tcd in &deduped {
            let key = tcd_key(tcd.toll_a_id, tcd.toll_b_id);
            if let Some(existing_entity) = existing_by_key.get(&key) {
                let mut to_update = tcd.clone();
                to_update.transport_stage_tcd_id = existing_entity.transport_stage_tcd_id;
                if let Err(e) = self.update(&to_update).await {
                    tracing::error!(
                        transport_trans_id,
                        transport_stage_tcd_id = existing_entity.transport_stage_tcd_id,
                        key = ?key,
                        error = %e,
                        "[BECT] TCD update failed"
                    );
                    return Err(e);
                }
            } else {
                if let Err(e) = self.insert(tcd).await {
                    tracing::error!(
                        transport_trans_id,
                        key = ?key,
                        error = %e,
                        "[BECT] TCD insert failed"
                    );
                    return Err(e);
                }
            }
        }

        // Xóa các TCD đã có nhưng không nằm trong danh sách cần lưu.
        for (key, existing_entity) in &existing_by_key {
            if !keys_to_save.contains(key) {
                if let Err(e) = self
                    .delete_by_id(existing_entity.transport_stage_tcd_id)
                    .await
                {
                    tracing::warn!(
                        transport_trans_id,
                        transport_stage_tcd_id = existing_entity.transport_stage_tcd_id,
                        key = ?key,
                        error = %e,
                        "[BECT] TCD delete (not in to_save) failed"
                    );
                }
            }
        }
        Ok(())
    }
}

impl Default for TransportTransStageTcdService {
    fn default() -> Self {
        Self::new()
    }
}
