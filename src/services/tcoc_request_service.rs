//! Service TCOC_REQUEST (cache-aside), get_by_id, save.

use crate::db::repositories::{TcocRequest, TcocRequestRepository};
use crate::db::Repository;
use crate::services::cache::{
    DistributedCache, MemoryCache, PlaceholderDistributedCache, SimpleMemoryCache,
};
use crate::services::service::{generate_cache_key, Service, ServiceError, DEFAULT_CACHE_TTL};
use std::sync::Arc;

/// TCOC_REQUEST service with cache-aside pattern.
pub struct TcocRequestService {
    repository: Arc<TcocRequestRepository>,
    memory_cache: Arc<SimpleMemoryCache<TcocRequest>>,
    distributed_cache: Arc<PlaceholderDistributedCache<TcocRequest>>,
}

impl TcocRequestService {
    pub fn new() -> Self {
        Self {
            repository: Arc::new(TcocRequestRepository::new()),
            memory_cache: Arc::new(SimpleMemoryCache::new()),
            distributed_cache: Arc::new(PlaceholderDistributedCache::new()),
        }
    }
}

impl Default for TcocRequestService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Service<TcocRequest, i64> for TcocRequestService {
    /// Get TCOC_REQUEST by ID with cache-aside pattern.
    async fn get_by_id(&self, id: i64) -> Result<Option<TcocRequest>, ServiceError> {
        let cache_key = generate_cache_key("tcoc_request", id);

        // Step 1: Check memory cache first
        if let Ok(Some(entity)) = self.memory_cache.get(&cache_key) {
            return Ok(Some(entity));
        }

        // Step 2: Check distributed cache
        if let Ok(Some(entity)) = self.distributed_cache.get(&cache_key).await {
            let entity_clone: TcocRequest = entity.clone();
            let _ = MemoryCache::<TcocRequest>::set(
                &*self.memory_cache,
                &cache_key,
                entity_clone,
                Some(DEFAULT_CACHE_TTL),
            );
            return Ok(Some(entity));
        }

        // Step 3: Query database (repository sync → spawn_blocking)
        let repo = self.repository.clone();
        match tokio::task::spawn_blocking(move || repo.find_by_id(id)).await {
            Ok(result) => match result {
                Ok(Some(entity)) => {
                    // Step 4: Store in both caches
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

    /// Save TCOC_REQUEST with write-through pattern.
    async fn save(&self, entity: &TcocRequest) -> Result<i64, ServiceError> {
        // Step 1: Try saving to database first
        let repo = self.repository.clone();
        let entity_clone = entity.clone();
        match tokio::task::spawn_blocking(move || repo.insert(&entity_clone)).await {
            Ok(result) => match result {
                Ok(id) => {
                    // Step 2: If DB save succeeded, update cache
                    let cache_key = generate_cache_key("tcoc_request", id);
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
                    // DB save failed: return error (do not write cache; retry may reuse entity from caller if needed).
                    tracing::warn!(error = %db_error, "[DB] TCOC_REQUEST save failed");
                    Err(ServiceError::DatabaseError(db_error))
                }
            },
            Err(e) => Err(ServiceError::Other(format!("Task join error: {}", e))),
        }
    }

    /// Update TCOC_REQUEST with write-through pattern.
    async fn update(&self, id: i64, entity: &TcocRequest) -> Result<bool, ServiceError> {
        // Step 1: Try updating in database first
        let repo = self.repository.clone();
        let entity_clone = entity.clone();
        match tokio::task::spawn_blocking(move || repo.update(id, &entity_clone)).await {
            Ok(result) => match result {
                Ok(success) => {
                    if success {
                        // Step 2: Invalidate caches
                        let cache_key = generate_cache_key("tcoc_request", id);
                        let _ = self.memory_cache.remove(&cache_key);
                        let _ = self.distributed_cache.remove(&cache_key).await;

                        // Optionally, update cache with new entity
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

    /// Delete TCOC_REQUEST and invalidate caches.
    async fn delete(&self, id: i64) -> Result<bool, ServiceError> {
        // Step 1: Try deleting from database first
        let repo = self.repository.clone();
        match tokio::task::spawn_blocking(move || repo.delete(id)).await {
            Ok(result) => match result {
                Ok(success) => {
                    if success {
                        let cache_key = generate_cache_key("tcoc_request", id);
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

impl TcocRequestService {
    /// Get latest TCOC_REQUEST by REQUEST_ID (used when saving TCOC_RESPONSE to compute PROCESS_DURATION).
    pub async fn find_by_request_id(
        &self,
        request_id: i64,
    ) -> Result<Option<TcocRequest>, ServiceError> {
        let repo = self.repository.clone();
        tokio::task::spawn_blocking(move || repo.find_by_request_id(request_id))
            .await
            .map_err(|e| ServiceError::Other(format!("Task join error: {}", e)))?
            .map_err(ServiceError::DatabaseError)
    }

    /// Save request with additional parameters (compatible with current code).
    pub async fn save_request(
        &self,
        fe_request: &crate::models::TCOCmessages::FE_REQUEST,
        conn_id: i32,
        data: &[u8],
        toll_id: Option<i64>,
        etag_id: Option<String>,
        lane_id: Option<i64>,
        plate: Option<String>,
        tid: Option<String>,
        node_id: Option<String>,
    ) -> Result<i64, ServiceError> {
        use crate::db::repositories::TcocRequest;
        use chrono::Utc;
        use hex;

        // Format datetime cho Altibase
        let request_datetime = crate::utils::now_utc_db_string();

        // Content = full hex of message (store fully for audit).
        let content = hex::encode(data);

        // toll_in/toll_out are set only by direction from lane (update_toll_in_toll_out_by_request_id after handler); not inferred from command_id.
        let toll_in: Option<i64> = None;
        let toll_out: Option<i64> = None;

        // TCOC: command_id from fe_request. Description sufficient for quick audit; max 250 chars.
        const DESC_MAX: usize = 250;
        let desc = format!(
            "conn={} req={} cmd=0x{:02X} toll={:?} lane={:?}",
            conn_id, fe_request.request_id, fe_request.command_id, toll_id, lane_id
        );
        let description: Option<String> = Some(if desc.len() > DESC_MAX {
            desc[..DESC_MAX].to_string()
        } else {
            desc
        });

        let tcoc_request = TcocRequest {
            id: None, // ID will be auto-assigned from sequence TCOC_REQUEST_LONG_SEQ
            request_id: Some(fe_request.request_id),
            command_id: Some(fe_request.command_id as i64),
            session_id: Some(fe_request.session_id),
            toll_id,
            etag_id,
            lane_id,
            plate,
            content: Some(content),
            description,
            request_datetime: Some(request_datetime),
            toll_in,
            toll_out,
            image_count: None,
            tid,
            node_id,
        };

        // Call save (write-through)
        self.save(&tcoc_request).await
    }

    /// Update TOLL_IN/TOLL_OUT by transaction direction (I/O) after handler completes.
    /// IN ("I"): only toll_in = toll_id. OUT ("O"): toll_in = station_in_for_out, toll_out = toll_id.
    /// When tcoc_request_id is Some(id), update that specific TCOC_REQUEST row (correct when request_id is reused e.g. retry); when None, find by request_id (legacy).
    pub async fn update_toll_in_toll_out_by_request_id(
        &self,
        request_id: i64,
        direction: Option<String>,
        station_in_for_out: Option<i64>,
        toll_id: Option<i64>,
        tcoc_request_id: Option<i64>,
    ) -> Result<bool, ServiceError> {
        let entity = if let Some(id) = tcoc_request_id {
            match self.get_by_id(id).await? {
                Some(e) => e,
                None => return Ok(false),
            }
        } else {
            let repo = self.repository.clone();
            let found = tokio::task::spawn_blocking(move || repo.find_by_request_id(request_id))
                .await
                .map_err(|e| ServiceError::Other(format!("Task join error: {}", e)))?
                .map_err(ServiceError::DatabaseError)?;
            match found {
                Some(e) => e,
                None => return Ok(false),
            }
        };
        let mut entity = entity;
        let id = match entity.id {
            Some(i) => i,
            None => return Ok(false),
        };
        let (toll_in, toll_out) = match direction.as_deref() {
            Some("I") => (toll_id, None),
            Some("O") => (station_in_for_out, toll_id),
            _ => (entity.toll_in, entity.toll_out),
        };
        entity.toll_in = toll_in;
        entity.toll_out = toll_out;
        self.update(id, &entity).await
    }
}
