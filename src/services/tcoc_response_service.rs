//! Service TCOC_RESPONSE (cache-aside), get_by_id, save.

use crate::db::repositories::{TcocResponse, TcocResponseRepository};
use crate::db::Repository;
use crate::services::cache::{
    DistributedCache, MemoryCache, PlaceholderDistributedCache, SimpleMemoryCache,
};
use crate::services::service::{generate_cache_key, Service, ServiceError, DEFAULT_CACHE_TTL};
use std::sync::Arc;

/// Service TCOC_RESPONSE với pattern cache-aside.
pub struct TcocResponseService {
    repository: Arc<TcocResponseRepository>,
    memory_cache: Arc<SimpleMemoryCache<TcocResponse>>,
    distributed_cache: Arc<PlaceholderDistributedCache<TcocResponse>>,
}

impl TcocResponseService {
    pub fn new() -> Self {
        Self {
            repository: Arc::new(TcocResponseRepository::new()),
            memory_cache: Arc::new(SimpleMemoryCache::new()),
            distributed_cache: Arc::new(PlaceholderDistributedCache::new()),
        }
    }
}

impl Default for TcocResponseService {
    fn default() -> Self {
        Self::new()
    }
}

impl TcocResponseService {
    /// Saves the response. PROCESS_DURATION = time from server starting to process the request to response ready (response_send_ms - request_received_ms).
    /// For performance evaluation: request_received_ms is captured in the processor right after parsing the FE header (before body parse and handler);
    /// response_send_ms is captured when the response is ready (before apply_post_handler). So process_duration reflects total processing time (parse + handler).
    pub async fn save_response(
        &self,
        request_id: i64,
        command_id: i32,
        session_id: i64,
        response_data: &[u8],
        status: Option<String>,
        ticket_id: Option<i64>,
        toll_id: Option<i64>,
        toll_in: Option<i64>,
        toll_out: Option<i64>,
        etag_id: Option<String>,
        lane_id: Option<i64>,
        plate: Option<String>,
        node_id: Option<String>,
        request_received_ms: i64,
        response_send_ms: i64,
    ) -> Result<i64, ServiceError> {
        let process_duration_ms = response_send_ms - request_received_ms;

        // Tạo entity
        use chrono::Utc;
        use hex;

        let response_datetime = crate::utils::now_utc_db_string();

        // Content = full hex của bản tin response (lưu đầy đủ để theo dõi/audit).
        let content = hex::encode(response_data);

        const DESC_MAX: usize = 250;
        let desc = format!(
            "req={} cmd=0x{:02X} toll={:?} in={:?} out={:?} status={:?}",
            request_id,
            command_id,
            toll_id,
            toll_in,
            toll_out,
            status.as_deref().unwrap_or("-")
        );
        let description = if desc.len() > DESC_MAX {
            desc[..DESC_MAX].to_string()
        } else {
            desc
        };

        let tcoc_response = TcocResponse {
            id: None, // ID sẽ được tự động lấy từ sequence TCOC_RESPONSE_LONG_SEQ
            request_id: Some(request_id),
            command_id: Some(command_id as i64),
            session_id: Some(session_id),
            toll_id,
            toll_in,
            toll_out,
            etag_id,
            lane_id,
            plate,
            content: Some(content),
            description: Some(description),
            response_datetime: Some(response_datetime),
            status,
            ticket_id,
            process_duration: Some(process_duration_ms),
            node_id,
        };

        // Gọi save với write-through pattern
        self.save(&tcoc_response).await
    }
}

#[async_trait::async_trait]
impl Service<TcocResponse, i64> for TcocResponseService {
    /// Lấy TCOC_RESPONSE theo ID với pattern cache-aside.
    async fn get_by_id(&self, id: i64) -> Result<Option<TcocResponse>, ServiceError> {
        let cache_key = generate_cache_key("tcoc_response", id);

        // Bước 1: Kiểm tra memory cache trước
        if let Ok(Some(entity)) = self.memory_cache.get(&cache_key) {
            return Ok(Some(entity));
        }

        // Bước 2: Kiểm tra distributed cache
        if let Ok(Some(entity)) = self.distributed_cache.get(&cache_key).await {
            let entity_clone: TcocResponse = entity.clone();
            let _ = MemoryCache::<TcocResponse>::set(
                &*self.memory_cache,
                &cache_key,
                entity_clone,
                Some(DEFAULT_CACHE_TTL),
            );
            return Ok(Some(entity));
        }

        // Bước 3: Truy vấn database
        let repo = self.repository.clone();
        match tokio::task::spawn_blocking(move || repo.find_by_id(id)).await {
            Ok(result) => match result {
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

    /// Lưu TCOC_RESPONSE với pattern write-through.
    async fn save(&self, entity: &TcocResponse) -> Result<i64, ServiceError> {
        // Bước 1: Thử lưu vào database trước
        let repo = self.repository.clone();
        let entity_clone = entity.clone();
        match tokio::task::spawn_blocking(move || repo.insert(&entity_clone)).await {
            Ok(result) => match result {
                Ok(id) => {
                    // Bước 2: Nếu lưu database thành công, cập nhật cache
                    let cache_key = generate_cache_key("tcoc_response", id);
                    let repo2 = self.repository.clone();
                    if let Ok(Ok(Some(updated_entity))) =
                        tokio::task::spawn_blocking(move || repo2.find_by_id(id)).await
                    {
                        let _ = self.memory_cache.set(
                            &cache_key,
                            updated_entity.clone(),
                            Some(DEFAULT_CACHE_TTL),
                        );
                        let _ = self
                            .distributed_cache
                            .set(&cache_key, updated_entity, Some(DEFAULT_CACHE_TTL))
                            .await;
                    }

                    Ok(id)
                }
                Err(db_error) => {
                    // Bước 3: Lưu DB thất bại (TODO: cache retry)
                    tracing::warn!(error = %db_error, "[DB] TCOC_RESPONSE save failed, entity saved to cache for retry");
                    // TODO: Implement cache-based retry mechanism
                    Err(ServiceError::DatabaseError(db_error))
                }
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Cập nhật TCOC_RESPONSE với pattern write-through.
    async fn update(&self, id: i64, entity: &TcocResponse) -> Result<bool, ServiceError> {
        // Bước 1: Thử cập nhật trong database trước
        let repo = self.repository.clone();
        let entity_clone = entity.clone();
        match tokio::task::spawn_blocking(move || repo.update(id, &entity_clone)).await {
            Ok(result) => match result {
                Ok(success) => {
                    if success {
                        // Bước 2: Invalidate caches
                        let cache_key = generate_cache_key("tcoc_response", id);
                        let _ = self.memory_cache.remove(&cache_key);
                        let _ = self.distributed_cache.remove(&cache_key).await;
                        let repo2 = self.repository.clone();
                        if let Ok(Ok(Some(updated_entity))) =
                            tokio::task::spawn_blocking(move || repo2.find_by_id(id)).await
                        {
                            let _ = self.memory_cache.set(
                                &cache_key,
                                updated_entity.clone(),
                                Some(DEFAULT_CACHE_TTL),
                            );
                            let _ = self
                                .distributed_cache
                                .set(&cache_key, updated_entity, Some(DEFAULT_CACHE_TTL))
                                .await;
                        }
                    }
                    Ok(success)
                }
                Err(db_error) => Err(ServiceError::DatabaseError(db_error)),
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Xóa TCOC_RESPONSE và invalidate caches.
    async fn delete(&self, id: i64) -> Result<bool, ServiceError> {
        // Bước 1: Thử xóa từ database trước
        let repo = self.repository.clone();
        match tokio::task::spawn_blocking(move || repo.delete(id)).await {
            Ok(result) => match result {
                Ok(success) => {
                    if success {
                        // Bước 2: Invalidate caches
                        let cache_key = generate_cache_key("tcoc_response", id);
                        let _ = self.memory_cache.remove(&cache_key);
                        let _ = self.distributed_cache.remove(&cache_key).await;
                    }
                    Ok(success)
                }
                Err(db_error) => Err(ServiceError::DatabaseError(db_error)),
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }
}
