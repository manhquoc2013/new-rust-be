//! Service TCOC_CONNECTION_SERVER: get_by_ip, get_by_id (cache-aside), save, update, delete.
//! get_by_ip: ưu tiên DB; when DB fails thì fallback cache/KeyDB để hệ thống không gián đoạn.

use crate::cache::data::connection_data_cache::{
    get_connection_server_by_ip, set_connection_server,
};
use crate::cache::data::db_retry::enqueue_tcoc_conn_server;
use crate::db::repositories::{TcocConnectionServer, TcocConnectionServerRepository};
use crate::db::Repository;
use crate::services::cache::{
    DistributedCache, MemoryCache, PlaceholderDistributedCache, SimpleMemoryCache,
};
use crate::services::service::{generate_cache_key, Service, ServiceError, DEFAULT_CACHE_TTL};
use std::sync::Arc;

/// Service TCOC_CONNECTION_SERVER với pattern cache-aside.
pub struct TcocConnectionServerService {
    repository: Arc<TcocConnectionServerRepository>,
    memory_cache: Arc<SimpleMemoryCache<TcocConnectionServer>>,
    distributed_cache: Arc<PlaceholderDistributedCache<TcocConnectionServer>>,
}

impl TcocConnectionServerService {
    pub fn new() -> Self {
        Self {
            repository: Arc::new(TcocConnectionServerRepository::new()),
            memory_cache: Arc::new(SimpleMemoryCache::new()),
            distributed_cache: Arc::new(PlaceholderDistributedCache::new()),
        }
    }
}

impl Default for TcocConnectionServerService {
    fn default() -> Self {
        Self::new()
    }
}

impl TcocConnectionServerService {
    /// Lấy TCOC_CONNECTION_SERVER theo địa chỉ IP. Prefer DB; when DB fails thì fallback cache/KeyDB.
    pub async fn get_by_ip(&self, ip: &str) -> Result<Option<TcocConnectionServer>, ServiceError> {
        let repo = self.repository.clone();
        let ip_str = ip.to_string();
        match tokio::task::spawn_blocking(move || repo.find_by_ip(&ip_str)).await {
            Ok(Ok(Some(entity))) => {
                set_connection_server(ip, &entity).await;
                Ok(Some(entity))
            }
            Ok(Ok(None)) => Ok(None),
            Ok(Err(e)) => {
                if let Some(entity) = get_connection_server_by_ip(ip).await {
                    tracing::debug!(ip = %ip, "[Cache] ConnectionServer fallback from cache/KeyDB");
                    Ok(Some(entity))
                } else {
                    Err(ServiceError::DatabaseError(e))
                }
            }
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Lấy tất cả bản ghi TCOC_CONNECTION_SERVER theo IP (nhiều record cùng IP). Prefer DB; lỗi thì fallback một bản ghi từ cache.
    pub async fn get_all_by_ip(&self, ip: &str) -> Result<Vec<TcocConnectionServer>, ServiceError> {
        let repo = self.repository.clone();
        let ip_str = ip.to_string();
        match tokio::task::spawn_blocking(move || repo.find_all_by_ip(&ip_str)).await {
            Ok(Ok(list)) if !list.is_empty() => {
                if let Some(ref first) = list.first() {
                    set_connection_server(ip, first).await;
                }
                Ok(list)
            }
            Ok(Ok(_)) => Ok(Vec::new()),
            Ok(Err(e)) => {
                if let Some(entity) = get_connection_server_by_ip(ip).await {
                    tracing::debug!(ip = %ip, "[Cache] ConnectionServer fallback from cache/KeyDB");
                    Ok(vec![entity])
                } else {
                    Err(ServiceError::DatabaseError(e))
                }
            }
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }
}

#[async_trait::async_trait]
impl Service<TcocConnectionServer, i64> for TcocConnectionServerService {
    /// Lấy TCOC_CONNECTION_SERVER theo ID (cache-aside).
    async fn get_by_id(&self, id: i64) -> Result<Option<TcocConnectionServer>, ServiceError> {
        let cache_key = generate_cache_key("tcoc_connection_server", id);

        if let Ok(Some(entity)) = self.memory_cache.get(&cache_key) {
            return Ok(Some(entity));
        }

        if let Ok(Some(entity)) = self.distributed_cache.get(&cache_key).await {
            let entity_clone: TcocConnectionServer = entity.clone();
            let _ = MemoryCache::<TcocConnectionServer>::set(
                &*self.memory_cache,
                &cache_key,
                entity_clone,
                Some(DEFAULT_CACHE_TTL),
            );
            return Ok(Some(entity));
        }

        let repo = self.repository.clone();
        match tokio::task::spawn_blocking(move || repo.find_by_id(id)).await {
            Ok(result) => match result {
                Ok(Some(entity)) => {
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

    /// Lưu TCOC_CONNECTION_SERVER với pattern write-through.
    async fn save(&self, entity: &TcocConnectionServer) -> Result<i64, ServiceError> {
        let repo = self.repository.clone();
        let entity_clone = entity.clone();
        match tokio::task::spawn_blocking(move || repo.insert(&entity_clone)).await {
            Ok(result) => match result {
                Ok(id) => {
                    let cache_key = generate_cache_key("tcoc_connection_server", id);
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
                    tracing::warn!(error = %db_error, "[DB] TcocConnectionServer save failed, enqueued for retry");
                    enqueue_tcoc_conn_server(None, entity).await;
                    Err(ServiceError::DatabaseError(db_error))
                }
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Cập nhật TCOC_CONNECTION_SERVER với pattern write-through.
    async fn update(&self, id: i64, entity: &TcocConnectionServer) -> Result<bool, ServiceError> {
        let repo = self.repository.clone();
        let entity_clone = entity.clone();
        match tokio::task::spawn_blocking(move || repo.update(id, &entity_clone)).await {
            Ok(result) => match result {
                Ok(success) => {
                    if success {
                        let cache_key = generate_cache_key("tcoc_connection_server", id);
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
                    tracing::warn!(id, error = %db_error, "[DB] TcocConnectionServer update failed, enqueued for retry");
                    enqueue_tcoc_conn_server(Some(id), entity).await;
                    Err(ServiceError::DatabaseError(db_error))
                }
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Delete TCOC_CONNECTION_SERVER and invalidate caches
    /// Xóa TCOC_CONNECTION_SERVER và invalidate caches
    async fn delete(&self, id: i64) -> Result<bool, ServiceError> {
        let repo = self.repository.clone();
        match tokio::task::spawn_blocking(move || repo.delete(id)).await {
            Ok(result) => match result {
                Ok(success) => {
                    if success {
                        let cache_key = generate_cache_key("tcoc_connection_server", id);
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
