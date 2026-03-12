//! Service TCOC_SESSION (cache-aside), get_by_id, save. Save lỗi thì ghi KeyDB để retry.

use crate::cache::data::db_retry::enqueue_tcoc_session;
use crate::db::repositories::{TcocSession, TcocSessionRepository};
use crate::db::Repository;
use crate::services::cache::{
    DistributedCache, MemoryCache, PlaceholderDistributedCache, SimpleMemoryCache,
};
use crate::services::service::{generate_cache_key, Service, ServiceError, DEFAULT_CACHE_TTL};
use std::sync::Arc;

/// Service TCOC_SESSIONS với pattern cache-aside.
pub struct TcocSessionService {
    repository: Arc<TcocSessionRepository>,
    memory_cache: Arc<SimpleMemoryCache<TcocSession>>,
    distributed_cache: Arc<PlaceholderDistributedCache<TcocSession>>,
}

impl TcocSessionService {
    pub fn new() -> Self {
        Self {
            repository: Arc::new(TcocSessionRepository::new()),
            memory_cache: Arc::new(SimpleMemoryCache::new()),
            distributed_cache: Arc::new(PlaceholderDistributedCache::new()),
        }
    }
}

impl Default for TcocSessionService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Service<TcocSession, i64> for TcocSessionService {
    /// Lấy TCOC_SESSION theo ID với pattern cache-aside.
    async fn get_by_id(&self, id: i64) -> Result<Option<TcocSession>, ServiceError> {
        let cache_key = generate_cache_key("tcoc_session", id);

        // Bước 1: Kiểm tra memory cache trước
        if let Ok(Some(entity)) = self.memory_cache.get(&cache_key) {
            return Ok(Some(entity));
        }

        // Bước 2: Kiểm tra distributed cache
        if let Ok(Some(entity)) = self.distributed_cache.get(&cache_key).await {
            // Lưu vào memory cache để truy cập nhanh hơn lần sau
            let entity_clone: TcocSession = entity.clone();
            let _ = MemoryCache::<TcocSession>::set(
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

    /// Lưu TCOC_SESSION với pattern write-through.
    async fn save(&self, entity: &TcocSession) -> Result<i64, ServiceError> {
        // Bước 1: Thử lưu vào database trước
        let repo = self.repository.clone();
        let entity_clone = entity.clone();
        let session_id = entity.session_id;
        match tokio::task::spawn_blocking(move || repo.insert(&entity_clone)).await {
            Ok(result) => match result {
                Ok(id) => {
                    // Bước 2: Nếu lưu database thành công, cập nhật/invalidate caches
                    let cache_key = generate_cache_key("tcoc_session", id);

                    // Cập nhật memory cache với entity mới
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
                    // Bước 3: Kiểm tra lỗi duplicate key (vi phạm unique constraint)
                    let error_msg = format!("{}", db_error);
                    let is_duplicate_key = error_msg.contains("State: 23000")
                        && (error_msg.contains("Native error: 69720")
                            || error_msg.contains("unique index")
                            || error_msg.contains("already exists"));

                    if is_duplicate_key && session_id > 0 {
                        // Bước 4: Nếu duplicate key và session_id đã set, thử lấy session đã tồn tại
                        tracing::info!(
                            session_id,
                            "[DB] TCOC_SESSION duplicate key, fetching existing"
                        );
                        let repo3 = self.repository.clone();
                        match tokio::task::spawn_blocking(move || repo3.find_by_id(session_id))
                            .await
                        {
                            Ok(Ok(Some(existing_session))) => {
                                // Session đã tồn tại, trả về ID và cập nhật cache
                                let cache_key = generate_cache_key("tcoc_session", session_id);
                                let _ = self.memory_cache.set(
                                    &cache_key,
                                    existing_session.clone(),
                                    Some(DEFAULT_CACHE_TTL),
                                );
                                let _ = self
                                    .distributed_cache
                                    .set(&cache_key, existing_session, Some(DEFAULT_CACHE_TTL))
                                    .await;
                                tracing::info!(session_id, "[DB] TCOC_SESSION already exists");
                                Ok(session_id)
                            }
                            Ok(Ok(None)) => {
                                // This shouldn't happen - duplicate key but session not found
                                // Điều này không nên xảy ra - duplicate key nhưng không tìm thấy session
                                tracing::warn!(
                                    session_id,
                                    "[DB] TCOC_SESSION duplicate key but session not found"
                                );
                                Err(ServiceError::DatabaseError(db_error))
                            }
                            Ok(Err(e)) => {
                                tracing::error!(session_id, error = %e, "[DB] TCOC_SESSION fetch existing failed");
                                Err(ServiceError::DatabaseError(db_error))
                            }
                            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
                        }
                    } else {
                        tracing::warn!(session_id, error = %db_error, "[DB] TCOC_SESSION save failed, enqueued for retry");
                        enqueue_tcoc_session(entity).await;
                        Err(ServiceError::DatabaseError(db_error))
                    }
                }
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Cập nhật TCOC_SESSION với pattern write-through.
    async fn update(&self, id: i64, entity: &TcocSession) -> Result<bool, ServiceError> {
        // Bước 1: Thử cập nhật trong database trước
        let repo = self.repository.clone();
        let entity_clone = entity.clone();
        match tokio::task::spawn_blocking(move || repo.update(id, &entity_clone)).await {
            Ok(result) => match result {
                Ok(success) => {
                    if success {
                        // Bước 2: Invalidate caches
                        let cache_key = generate_cache_key("tcoc_session", id);
                        let _ = self.memory_cache.remove(&cache_key);
                        let _ = self.distributed_cache.remove(&cache_key).await;

                        // Optionally, update cache with new entity
                        // Tùy chọn, cập nhật cache với entity mới
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

    /// Delete TCOC_SESSION and invalidate caches
    /// Xóa TCOC_SESSION và invalidate caches
    async fn delete(&self, id: i64) -> Result<bool, ServiceError> {
        // Bước 1: Thử xóa từ database trước
        let repo = self.repository.clone();
        match tokio::task::spawn_blocking(move || repo.delete(id)).await {
            Ok(result) => match result {
                Ok(success) => {
                    if success {
                        // Bước 2: Invalidate caches
                        let cache_key = generate_cache_key("tcoc_session", id);
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
