//! Service TCOC_USER (cache-aside), get_by_id, get_by_username_and_toll_id, save.
//! get_by_username_and_toll_id: ưu tiên DB; when DB fails thì fallback cache/KeyDB. save/update lỗi thì ghi KeyDB để retry.

use crate::cache::data::connection_data_cache::{get_connection_user, set_connection_user};
use crate::cache::data::db_retry::enqueue_tcoc_user;
use crate::db::repositories::{TcocUser, TcocUserRepository};
use crate::db::Repository;
use crate::services::cache::{
    DistributedCache, MemoryCache, PlaceholderDistributedCache, SimpleMemoryCache,
};
use crate::services::service::{generate_cache_key, Service, ServiceError, DEFAULT_CACHE_TTL};
use std::sync::Arc;

/// Service TCOC_USERS với pattern cache-aside.
pub struct TcocUserService {
    repository: Arc<TcocUserRepository>,
    memory_cache: Arc<SimpleMemoryCache<TcocUser>>,
    distributed_cache: Arc<PlaceholderDistributedCache<TcocUser>>,
}

impl TcocUserService {
    pub fn new() -> Self {
        Self {
            repository: Arc::new(TcocUserRepository::new()),
            memory_cache: Arc::new(SimpleMemoryCache::new()),
            distributed_cache: Arc::new(PlaceholderDistributedCache::new()),
        }
    }
}

impl Default for TcocUserService {
    fn default() -> Self {
        Self::new()
    }
}

impl TcocUserService {
    /// Lấy TCOC_USER theo username và toll_id. Prefer DB; when DB fails thì fallback cache/KeyDB.
    pub async fn get_by_username_and_toll_id(
        &self,
        username: &str,
        toll_id: i64,
    ) -> Result<Option<TcocUser>, ServiceError> {
        let repo = self.repository.clone();
        let username_str = username.to_string();
        match tokio::task::spawn_blocking(move || {
            repo.find_by_username_and_toll_id(&username_str, toll_id)
        })
        .await
        {
            Ok(Ok(Some(ref user))) => {
                set_connection_user(username, toll_id, user).await;
                Ok(Some(user.clone()))
            }
            Ok(Ok(None)) => Ok(None),
            Ok(Err(e)) => {
                if let Some(user) = get_connection_user(username, toll_id).await {
                    tracing::debug!(username = %username, toll_id, "[Cache] ConnectionUser fallback from cache/KeyDB");
                    Ok(Some(user))
                } else {
                    Err(ServiceError::DatabaseError(e))
                }
            }
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }
}

#[async_trait::async_trait]
impl Service<TcocUser, i64> for TcocUserService {
    /// Lấy TCOC_USER theo ID với pattern cache-aside.
    async fn get_by_id(&self, id: i64) -> Result<Option<TcocUser>, ServiceError> {
        let cache_key = generate_cache_key("tcoc_user", id);

        // Bước 1: Kiểm tra memory cache trước
        if let Ok(Some(entity)) = self.memory_cache.get(&cache_key) {
            return Ok(Some(entity));
        }

        // Bước 2: Kiểm tra distributed cache
        if let Ok(Some(entity)) = self.distributed_cache.get(&cache_key).await {
            // Lưu vào memory cache để truy cập nhanh hơn lần sau
            let entity_clone: TcocUser = entity.clone();
            let _ = MemoryCache::<TcocUser>::set(
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

    /// Lưu TCOC_USER với pattern write-through.
    async fn save(&self, entity: &TcocUser) -> Result<i64, ServiceError> {
        // Bước 1: Thử lưu vào database trước
        let repo = self.repository.clone();
        let entity_clone = entity.clone();
        match tokio::task::spawn_blocking(move || repo.insert(&entity_clone)).await {
            Ok(result) => match result {
                Ok(id) => {
                    // Bước 2: Nếu lưu database thành công, cập nhật/invalidate caches
                    let cache_key = generate_cache_key("tcoc_user", id);

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
                    tracing::warn!(error = %db_error, "[DB] TCOC_USER save failed, enqueued for retry");
                    enqueue_tcoc_user(None, entity).await;
                    Err(ServiceError::DatabaseError(db_error))
                }
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Cập nhật TCOC_USER với pattern write-through.
    async fn update(&self, id: i64, entity: &TcocUser) -> Result<bool, ServiceError> {
        // Bước 1: Thử cập nhật trong database trước
        let repo = self.repository.clone();
        let entity_clone = entity.clone();
        match tokio::task::spawn_blocking(move || repo.update(id, &entity_clone)).await {
            Ok(result) => match result {
                Ok(success) => {
                    if success {
                        // Bước 2: Invalidate caches
                        let cache_key = generate_cache_key("tcoc_user", id);
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
                Err(db_error) => {
                    tracing::warn!(id, error = %db_error, "[DB] TCOC_USER update failed, enqueued for retry");
                    enqueue_tcoc_user(Some(id), entity).await;
                    Err(ServiceError::DatabaseError(db_error))
                }
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Delete TCOC_USER and invalidate caches
    /// Xóa TCOC_USER và invalidate caches
    async fn delete(&self, id: i64) -> Result<bool, ServiceError> {
        // Bước 1: Thử xóa từ database trước
        let repo = self.repository.clone();
        match tokio::task::spawn_blocking(move || repo.delete(id)).await {
            Ok(result) => match result {
                Ok(success) => {
                    if success {
                        // Bước 2: Invalidate caches
                        let cache_key = generate_cache_key("tcoc_user", id);
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
