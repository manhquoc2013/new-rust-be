//! Service TRANSPORT_TRANSACTION_STAGE (cache-aside), get_by_id, save.

use crate::constants::verify_after_save_service;
use crate::db::repositories::{TransportTransactionStage, TransportTransactionStageRepository};
use crate::db::Repository;
use crate::models::ETDR::get_etdr_cache;
use crate::services::cache::{
    DistributedCache, MemoryCache, PlaceholderDistributedCache, SimpleMemoryCache,
};
use crate::services::service::{generate_cache_key, Service, ServiceError, DEFAULT_CACHE_TTL};
use crate::services::ticket_id_service::TicketIdService;
use std::sync::Arc;

/// Service TRANSPORT_TRANSACTION_STAGE với pattern cache-aside.
pub struct TransportTransactionStageService {
    repository: Arc<TransportTransactionStageRepository>,
    memory_cache: Arc<SimpleMemoryCache<TransportTransactionStage>>,
    distributed_cache: Arc<PlaceholderDistributedCache<TransportTransactionStage>>,
}

impl TransportTransactionStageService {
    pub fn new() -> Self {
        Self {
            repository: Arc::new(TransportTransactionStageRepository::new()),
            memory_cache: Arc::new(SimpleMemoryCache::new()),
            distributed_cache: Arc::new(PlaceholderDistributedCache::new()),
        }
    }

    /// Lấy ticket_id cho BECT checkin. Bắc cầu qua TicketIdService (dùng chung với BOO, singleton).
    pub async fn get_ticket_id_for_bect_checkin(&self, request_id: i64) -> i64 {
        TicketIdService::instance()
            .get_next_ticket_id_for_checkin(request_id)
            .await
    }
}

impl Default for TransportTransactionStageService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Service<TransportTransactionStage, i64> for TransportTransactionStageService {
    /// Lấy TRANSPORT_TRANSACTION_STAGE theo ID với pattern cache-aside.
    async fn get_by_id(&self, id: i64) -> Result<Option<TransportTransactionStage>, ServiceError> {
        let cache_key = generate_cache_key("transport_transaction_stage", id);

        // Bước 1: Kiểm tra memory cache trước
        if let Ok(Some(entity)) = self.memory_cache.get(&cache_key) {
            return Ok(Some(entity));
        }

        // Bước 2: Kiểm tra distributed cache
        if let Ok(Some(entity)) = self.distributed_cache.get(&cache_key).await {
            let entity_clone: TransportTransactionStage = entity.clone();
            let _ = MemoryCache::<TransportTransactionStage>::set(
                &*self.memory_cache,
                &cache_key,
                entity_clone,
                Some(DEFAULT_CACHE_TTL),
            );
            return Ok(Some(entity));
        }

        // Bước 3: Truy vấn database (repository sync → spawn_blocking)
        let repo = self.repository.clone();
        let start = std::time::Instant::now();
        let result = tokio::task::spawn_blocking(move || repo.find_by_id(id)).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let id_s = id.to_string();
        crate::logging::log_db_time(
            "GET",
            elapsed_ms,
            Some("TRANSPORT_TRANSACTION_STAGE"),
            Some(id_s.as_str()),
        );
        match result {
            Ok(inner) => match inner {
                Ok(Some(entity)) => {
                    // Bước 4: Lưu vào cả hai cache
                    let _ =
                        self.memory_cache
                            .set(&cache_key, entity.clone(), Some(DEFAULT_CACHE_TTL));
                    let _ = self
                        .distributed_cache
                        .set(&cache_key, entity.clone(), Some(DEFAULT_CACHE_TTL))
                        .await;
                    Ok(Some(entity))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(ServiceError::DatabaseError(e)),
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Lưu TRANSPORT_TRANSACTION_STAGE với pattern write-through. Verify record tồn tại sau save rồi mới update cache.
    async fn save(&self, entity: &TransportTransactionStage) -> Result<i64, ServiceError> {
        // Bước 1: Thử lưu vào database trước
        let repo = self.repository.clone();
        let entity_clone = entity.clone();

        let start = std::time::Instant::now();
        let transport_trans_id = match tokio::task::spawn_blocking(move || {
            repo.insert(&entity_clone)
        })
        .await
        {
            Ok(result) => match result {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!(
                        transport_trans_id = entity.transport_trans_id,
                        request_id = ?entity.request_id,
                        checkin_toll_id = ?entity.checkin_toll_id,
                        checkin_lane_id = ?entity.checkin_lane_id,
                        ticket_in_id = ?entity.ticket_in_id,
                        error = %e,
                        "[DB] TRANSPORT_TRANSACTION_STAGE save failed"
                    );
                    tracing::debug!(entity = ?entity, "[DB] TRANSPORT_TRANSACTION_STAGE full entity");
                    return Err(ServiceError::DatabaseError(e));
                }
            },
            Err(e) => return Err(ServiceError::Other(format!("Task join error: {}", e))),
        };
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let id_s = transport_trans_id.to_string();
        crate::logging::log_db_time(
            "INSERT",
            elapsed_ms,
            Some("TRANSPORT_TRANSACTION_STAGE"),
            Some(id_s.as_str()),
        );

        // Bước 2: Xác minh record tồn tại trong DB (đảm bảo commit đã visible)
        // Retry với exponential backoff để xử lý race condition trong môi trường đa luồng
        let repo_verify = self.repository.clone();
        let mut verified_entity_opt = None;

        for verify_retry in 0..=verify_after_save_service::MAX_RETRIES {
            match tokio::task::spawn_blocking({
                let repo_clone = repo_verify.clone();
                let id = transport_trans_id;
                move || repo_clone.find_by_id(id)
            })
            .await
            {
                Ok(Ok(Some(record))) => {
                    verified_entity_opt = Some(record);
                    if verify_retry > 0 {
                        tracing::debug!(
                            transport_trans_id,
                            verify_retry,
                            "[DB] TRANSPORT_TRANSACTION_STAGE verified after retry"
                        );
                    }
                    break;
                }
                Ok(Ok(None)) => {
                    if verify_retry < verify_after_save_service::MAX_RETRIES {
                        let delay_ms = verify_after_save_service::INITIAL_VERIFY_DELAY_MS
                            * (1 << verify_retry);
                        tracing::debug!(
                            transport_trans_id,
                            delay_ms,
                            attempt = verify_retry + 1,
                            max = verify_after_save_service::MAX_RETRIES,
                            "[DB] record not visible, retrying"
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                    } else {
                        tracing::warn!(transport_trans_id, retries = verify_after_save_service::MAX_RETRIES, "[DB] TRANSPORT_TRANSACTION_STAGE not found after verify retries (race?)");
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!(transport_trans_id, error = %e, "[DB] verify TRANSPORT_TRANSACTION_STAGE error, continuing");
                    break;
                }
                Err(e) => {
                    tracing::warn!(transport_trans_id, error = %e, "[DB] verify task join error, continuing");
                    break;
                }
            }
        }

        // Bước 3: Cập nhật cache với entity đã verify hoặc entity gốc
        let entity_for_cache = if let Some(verified) = verified_entity_opt {
            verified
        } else {
            // Fallback: use original entity with transport_trans_id set
            let mut updated_entity = entity.clone();
            updated_entity.transport_trans_id = transport_trans_id;
            updated_entity
        };

        let cache_key = generate_cache_key("transport_transaction_stage", transport_trans_id);
        let _ = self.memory_cache.set(
            &cache_key,
            entity_for_cache.clone(),
            Some(DEFAULT_CACHE_TTL),
        );
        let _ = self
            .distributed_cache
            .set(&cache_key, entity_for_cache, Some(DEFAULT_CACHE_TTL))
            .await;

        Ok(transport_trans_id)
    }

    /// Cập nhật TRANSPORT_TRANSACTION_STAGE với pattern write-through.
    async fn update(
        &self,
        id: i64,
        entity: &TransportTransactionStage,
    ) -> Result<bool, ServiceError> {
        // Bước 1: Thử cập nhật trong database trước
        let repo = self.repository.clone();
        let entity_clone = entity.clone();

        let start = std::time::Instant::now();
        let update_result =
            tokio::task::spawn_blocking(move || repo.update(id, &entity_clone)).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let id_s = id.to_string();
        crate::logging::log_db_time(
            "UPDATE",
            elapsed_ms,
            Some("TRANSPORT_TRANSACTION_STAGE"),
            Some(id_s.as_str()),
        );
        match update_result {
            Ok(result) => match result {
                Ok(updated) => {
                    // Bước 2: Nếu thành công, cập nhật/invalidate caches
                    let cache_key = generate_cache_key("transport_transaction_stage", id);
                    let _ =
                        self.memory_cache
                            .set(&cache_key, entity.clone(), Some(DEFAULT_CACHE_TTL));
                    let _ = self
                        .distributed_cache
                        .set(&cache_key, entity.clone(), Some(DEFAULT_CACHE_TTL))
                        .await;
                    Ok(updated)
                }
                Err(e) => {
                    tracing::error!(id, error = %e, "[DB] TRANSPORT_TRANSACTION_STAGE update failed");
                    Err(ServiceError::DatabaseError(e))
                }
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Xóa TRANSPORT_TRANSACTION_STAGE và invalidate cache.
    async fn delete(&self, id: i64) -> Result<bool, ServiceError> {
        // Bước 1: Thử xóa từ database trước
        let repo = self.repository.clone();

        let start = std::time::Instant::now();
        let delete_result = tokio::task::spawn_blocking(move || repo.delete(id)).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let id_s = id.to_string();
        crate::logging::log_db_time(
            "DELETE",
            elapsed_ms,
            Some("TRANSPORT_TRANSACTION_STAGE"),
            Some(id_s.as_str()),
        );
        match delete_result {
            Ok(result) => match result {
                Ok(deleted) => {
                    // Bước 2: Nếu thành công, invalidate caches
                    let cache_key = generate_cache_key("transport_transaction_stage", id);
                    let _ = self.memory_cache.remove(&cache_key);
                    let _ = self.distributed_cache.remove(&cache_key).await;
                    Ok(deleted)
                }
                Err(e) => {
                    tracing::error!(id, error = %e, "[DB] TRANSPORT_TRANSACTION_STAGE delete failed");
                    Err(ServiceError::DatabaseError(e))
                }
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }
}

impl TransportTransactionStageService {
    /// Get record theo TICKET_IN_ID (bất kể trạng thái). Dùng cho idempotent retry và tránh double insert cùng ticket_in_id.
    pub async fn get_by_ticket_in_id(
        &self,
        ticket_in_id: i64,
    ) -> Result<Option<TransportTransactionStage>, ServiceError> {
        if ticket_in_id == 0 {
            return Ok(None);
        }
        let repo = self.repository.clone();
        let start = std::time::Instant::now();
        let result =
            tokio::task::spawn_blocking(move || repo.find_by_ticket_in_id(ticket_in_id)).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        crate::logging::log_db_time(
            "GET",
            elapsed_ms,
            Some("TRANSPORT_TRANSACTION_STAGE"),
            Some("by_ticket_in_id"),
        );
        match result {
            Ok(inner) => inner.map_err(ServiceError::DatabaseError),
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Get record latest by etag (theo thời gian checkIn). Handler kiểm tra điều kiện (vd. chưa checkout) nếu cần.
    pub async fn find_latest_by_etag(
        &self,
        etag: &str,
    ) -> Result<Option<TransportTransactionStage>, ServiceError> {
        let repo = self.repository.clone();
        let etag_owned = etag.to_string();

        let start = std::time::Instant::now();
        let result =
            tokio::task::spawn_blocking(move || repo.find_latest_by_etag(&etag_owned)).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        crate::logging::log_db_time(
            "GET",
            elapsed_ms,
            Some("TRANSPORT_TRANSACTION_STAGE"),
            Some("by_etag"),
        );
        match result {
            Ok(inner) => match inner {
                Ok(Some(entity)) => {
                    let cache_key = generate_cache_key(
                        "transport_transaction_stage",
                        entity.transport_trans_id,
                    );
                    let _ =
                        self.memory_cache
                            .set(&cache_key, entity.clone(), Some(DEFAULT_CACHE_TTL));
                    let _ = self
                        .distributed_cache
                        .set(&cache_key, entity.clone(), Some(DEFAULT_CACHE_TTL))
                        .await;
                    Ok(Some(entity))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(ServiceError::DatabaseError(e)),
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Cập nhật thông tin checkout cho BECT (chỉ các field cần thiết). status: DB 1 = success, 0 = error.
    pub async fn update_checkout_info(
        &self,
        transport_trans_id: i64,
        checkout_toll_id: i64,
        checkout_lane_id: i64,
        checkout_plate: &str,
        total_amount: i64,
        checkout_datetime: &str,
        last_update: &str,
        charge_status: &str,
        charge_type: &str,
        vehicle_type: &str,
        turning_code: Option<&str>,
        checkout_tid: Option<&str>,
        charge_in: &str,
        account_id: i64,
        charge_in_status: Option<&str>,
        checkout_status: Option<&str>,
        account_trans_id: Option<i64>,
        checkout_plate_status: Option<&str>,
        checkout_channel: Option<i64>,
        checkout_pass: Option<&str>,
        checkout_pass_reason_id: Option<&str>,
        checkout_img_count: Option<i32>,
        status: Option<&str>,
    ) -> Result<bool, ServiceError> {
        let repo = self.repository.clone();
        let checkout_plate_owned = checkout_plate.to_string();
        let checkout_datetime_owned = checkout_datetime.to_string();
        let last_update_owned = last_update.to_string();
        let charge_status_owned = charge_status.to_string();
        let charge_type_owned = charge_type.to_string();
        let vehicle_type_owned = vehicle_type.to_string();
        let charge_in_owned = charge_in.to_string();
        let turning_code_owned = turning_code.map(|s| s.to_string());
        let checkout_tid_owned = checkout_tid.map(|s| s.to_string());
        let charge_in_status_owned = charge_in_status.map(|s| s.to_string());
        let checkout_status_owned = checkout_status.map(|s| s.to_string());
        let checkout_plate_status_owned = checkout_plate_status.map(|s| s.to_string());
        let checkout_pass_reason_id_owned = checkout_pass_reason_id.map(|s| s.to_string());
        let checkout_pass_owned = checkout_pass.map(|s| s.to_string());
        let status_owned = status.map(|s| s.to_string());

        let start = std::time::Instant::now();
        let update_result = tokio::task::spawn_blocking(move || {
            repo.update_checkout_info(
                transport_trans_id,
                checkout_toll_id,
                checkout_lane_id,
                &checkout_plate_owned,
                total_amount,
                &checkout_datetime_owned,
                &last_update_owned,
                &charge_status_owned,
                &charge_type_owned,
                &vehicle_type_owned,
                turning_code_owned.as_deref(),
                checkout_tid_owned.as_deref(),
                &charge_in_owned,
                account_id,
                charge_in_status_owned.as_deref(),
                checkout_status_owned.as_deref(),
                account_trans_id,
                checkout_plate_status_owned.as_deref(),
                checkout_channel,
                checkout_pass_owned.as_deref(),
                checkout_pass_reason_id_owned.as_deref(),
                checkout_img_count,
                status_owned.as_deref(),
            )
        })
        .await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let id_s = transport_trans_id.to_string();
        crate::logging::log_db_time(
            "UPDATE",
            elapsed_ms,
            Some("TRANSPORT_TRANSACTION_STAGE"),
            Some(id_s.as_str()),
        );
        match update_result {
            Ok(result) => match result {
                Ok(updated) => {
                    if updated {
                        // Invalidate cache to force reload on next access
                        let cache_key =
                            generate_cache_key("transport_transaction_stage", transport_trans_id);
                        let _ = self.memory_cache.remove(&cache_key);
                        let _ = self.distributed_cache.remove(&cache_key).await;
                    }
                    Ok(updated)
                }
                Err(e) => {
                    tracing::error!(transport_trans_id, error = %e, "[DB] TRANSPORT_TRANSACTION_STAGE update_checkout_info failed");
                    Err(ServiceError::DatabaseError(e))
                }
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Cập nhật checkout commit (BECT): set CHECKOUT_COMMIT_DATETIME, CHARGE_STATUS, v.v.
    pub async fn update_checkout_commit(
        &self,
        transport_trans_id: i64,
        checkout_commit_datetime: &str,
        checkout_img_count: i32,
        last_update: &str,
        charge_status: &str,
        checkout_status: Option<&str>,
        fe_reason_id: Option<i64>,
        checkout_datetime: Option<&str>,
        img_count: Option<i32>,
        checkout_plate_status: Option<&str>,
        checkout_plate: Option<&str>,
        sync_status: Option<i64>,
    ) -> Result<bool, ServiceError> {
        let repo = self.repository.clone();
        let checkout_commit_datetime_owned = checkout_commit_datetime.to_string();
        let last_update_owned = last_update.to_string();
        let charge_status_owned = charge_status.to_string();
        let checkout_status_owned = checkout_status.map(|s| s.to_string());
        let checkout_datetime_owned = checkout_datetime.map(|s| s.to_string());
        let checkout_plate_status_owned = checkout_plate_status.map(|s| s.to_string());
        let checkout_plate_owned = checkout_plate.map(|s| s.to_string());

        let start = std::time::Instant::now();
        let update_result = tokio::task::spawn_blocking(move || {
            repo.update_checkout_commit(
                transport_trans_id,
                &checkout_commit_datetime_owned,
                checkout_img_count,
                &last_update_owned,
                &charge_status_owned,
                checkout_status_owned.as_deref(),
                fe_reason_id,
                checkout_datetime_owned.as_deref(),
                img_count,
                checkout_plate_status_owned.as_deref(),
                checkout_plate_owned.as_deref(),
                sync_status,
            )
        })
        .await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let id_s = transport_trans_id.to_string();
        crate::logging::log_db_time(
            "UPDATE",
            elapsed_ms,
            Some("TRANSPORT_TRANSACTION_STAGE"),
            Some(id_s.as_str()),
        );
        match update_result {
            Ok(result) => match result {
                Ok(updated) => {
                    if updated {
                        let cache_key =
                            generate_cache_key("transport_transaction_stage", transport_trans_id);
                        let _ = self.memory_cache.remove(&cache_key);
                        let _ = self.distributed_cache.remove(&cache_key).await;
                    }
                    Ok(updated)
                }
                Err(e) => {
                    tracing::error!(transport_trans_id, error = %e, "[DB] TRANSPORT_TRANSACTION_STAGE update_checkout_commit failed");
                    Err(ServiceError::DatabaseError(e))
                }
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }
}
