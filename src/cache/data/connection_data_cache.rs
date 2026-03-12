//! Cache data for CONNECT: server config (theo IP), user (theo username+toll_id).
//! Load flow same as other caches: DB trước; DB lỗi và KeyDB có sẵn thì fallback KeyDB (ghi memory + Moka).
//! In-memory layer uses DashMap for concurrent get/set when cache is large.

use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use r2d2::Pool;

use crate::cache::config::{cache_manager::CacheManager, cache_prefix::CachePrefix};
use crate::db::repositories::{
    TcocConnectionServer, TcocConnectionServerRepository, TcocUser, TcocUserRepository,
};

use crate::configs::pool_factory::OdbcConnectionManager;

/// Bộ nhớ in-process: IP -> TcocConnectionServer. DashMap for concurrent access.
static CONNECTION_SERVER_MEMORY: OnceLock<DashMap<String, TcocConnectionServer>> = OnceLock::new();

/// Bộ nhớ in-process: key "username:toll_id" -> TcocUser. DashMap for concurrent access.
static CONNECTION_USER_MEMORY: OnceLock<DashMap<String, TcocUser>> = OnceLock::new();

/// CacheManager dùng cho KeyDB khi get_connection_server_by_ip / get_connection_user (fallback).
static CACHE: OnceLock<Arc<CacheManager>> = OnceLock::new();

fn server_memory() -> &'static DashMap<String, TcocConnectionServer> {
    CONNECTION_SERVER_MEMORY.get_or_init(DashMap::new)
}

fn user_memory() -> &'static DashMap<String, TcocUser> {
    CONNECTION_USER_MEMORY.get_or_init(DashMap::new)
}

/// Key lookup user: username:toll_id (username không được chứa ':').
fn user_cache_key(username: &str, toll_id: i64) -> String {
    format!("{}:{}", username, toll_id)
}

/// Load toàn bộ TCOC_CONNECTION_SERVER từ DB (hoặc KeyDB when DB fails) vào memory + Moka/KeyDB.
/// Luồng giống các cache khác: DB trước; DB lỗi và `load_from_keydb` thì fallback KeyDB (ghi vào memory + Moka, không ghi ngược KeyDB).
/// `pool_opt`: Option vì MEDIATION may not be ready lúc khởi động (init DB lỗi); khi None to omit DB.
pub async fn get_connection_server_cache(
    pool_opt: Option<Arc<Pool<OdbcConnectionManager>>>,
    cache: Arc<CacheManager>,
    load_from_keydb: bool,
) {
    if CACHE.get().is_none() {
        let _ = CACHE.set(cache.clone());
    }

    let result = if let Some(pool) = pool_opt {
        match tokio::task::spawn_blocking(move || {
            TcocConnectionServerRepository::find_all_with_pool(&pool)
        })
        .await
        {
            Ok(db_result) => db_result,
            Err(join_err) => Err(crate::db::error::DbError::ExecutionError(
                join_err.to_string(),
            )),
        }
    } else {
        tracing::warn!("[Cache] ConnectionServer skip DB (pool not available), will use KeyDB fallback if enabled");
        Err(crate::db::error::DbError::ExecutionError(
            "DB pool not available".to_string(),
        ))
    };

    match result {
        Ok(list) => {
            let prefix = CachePrefix::ConnectionServer.as_str();
            let cache_data: Vec<(String, TcocConnectionServer)> = list
                .into_iter()
                .map(|e| (format!("{}:{}", prefix, e.ip), e))
                .collect();
            let n = cache_data.len();
            cache.atomic_reload_prefix(prefix, cache_data.clone()).await;
            let mem = server_memory();
            mem.clear();
            for (_, v) in cache_data {
                let ip = v.ip.clone();
                mem.insert(ip, v);
            }
            if load_from_keydb {
                tracing::info!(
                    count = n,
                    "[Cache] ConnectionServer loaded from DB, synced to KeyDB"
                );
            } else {
                tracing::info!(count = n, "[Cache] ConnectionServer loaded from DB");
            }
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "[Cache] ConnectionServer load from DB failed");
        }
    }

    if load_from_keydb {
        let prefix = CachePrefix::ConnectionServer.as_str();
        let keydb_loaded = cache
            .load_prefix_from_keydb::<TcocConnectionServer>(prefix)
            .await;
        if keydb_loaded.is_empty() {
            tracing::warn!("[Cache] ConnectionServer fallback KeyDB empty, keeping empty");
        } else {
            cache
                .atomic_reload_prefix(prefix, keydb_loaded.clone())
                .await;
            let mem = server_memory();
            mem.clear();
            for (key, ref dto) in &keydb_loaded {
                if let Some(ip) = key.strip_prefix(&format!("{}:", prefix)) {
                    mem.insert(ip.to_string(), dto.clone());
                }
            }
            tracing::info!(
                count = keydb_loaded.len(),
                "[Cache] ConnectionServer loaded from KeyDB (DB failed, fallback)"
            );
        }
    }
}

/// Load toàn bộ TCOC_USERS từ DB (hoặc KeyDB when DB fails) vào memory + Moka/KeyDB.
/// Luồng giống các cache khác: DB trước; DB lỗi và `load_from_keydb` thì fallback KeyDB (ghi vào memory + Moka).
/// `pool_opt`: Option vì MEDIATION may not be ready lúc khởi động.
pub async fn get_connection_user_cache(
    pool_opt: Option<Arc<Pool<OdbcConnectionManager>>>,
    cache: Arc<CacheManager>,
    load_from_keydb: bool,
) {
    if CACHE.get().is_none() {
        let _ = CACHE.set(cache.clone());
    }
    let result = if let Some(pool) = pool_opt {
        match tokio::task::spawn_blocking(move || TcocUserRepository::find_all_with_pool(&pool))
            .await
        {
            Ok(db_result) => db_result,
            Err(join_err) => Err(crate::db::error::DbError::ExecutionError(
                join_err.to_string(),
            )),
        }
    } else {
        tracing::warn!("[Cache] ConnectionUser skip DB (pool not available), will use KeyDB fallback if enabled");
        Err(crate::db::error::DbError::ExecutionError(
            "DB pool not available".to_string(),
        ))
    };

    match result {
        Ok(list) => {
            let prefix = CachePrefix::ConnectionUser.as_str();
            let cache_data: Vec<(String, TcocUser)> = list
                .into_iter()
                .map(|e| {
                    let key = user_cache_key(&e.user_name, e.toll_id.unwrap_or(0));
                    (format!("{}:{}", prefix, key), e)
                })
                .collect();
            let n = cache_data.len();
            cache.atomic_reload_prefix(prefix, cache_data.clone()).await;
            let mem = user_memory();
            mem.clear();
            for (_, v) in cache_data {
                let key = user_cache_key(&v.user_name, v.toll_id.unwrap_or(0));
                mem.insert(key, v);
            }
            if load_from_keydb {
                tracing::info!(
                    count = n,
                    "[Cache] ConnectionUser loaded from DB, synced to KeyDB"
                );
            } else {
                tracing::info!(count = n, "[Cache] ConnectionUser loaded from DB");
            }
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "[Cache] ConnectionUser load from DB failed");
        }
    }

    if load_from_keydb {
        let prefix = CachePrefix::ConnectionUser.as_str();
        let keydb_loaded = cache.load_prefix_from_keydb::<TcocUser>(prefix).await;
        if keydb_loaded.is_empty() {
            tracing::warn!("[Cache] ConnectionUser fallback KeyDB empty, keeping empty");
        } else {
            cache
                .atomic_reload_prefix(prefix, keydb_loaded.clone())
                .await;
            let mem = user_memory();
            mem.clear();
            for (_, ref dto) in &keydb_loaded {
                let lookup_key = user_cache_key(&dto.user_name, dto.toll_id.unwrap_or(0));
                mem.insert(lookup_key, dto.clone());
            }
            tracing::info!(
                count = keydb_loaded.len(),
                "[Cache] ConnectionUser loaded from KeyDB (DB failed, fallback)"
            );
        }
    }
}

/// Lấy server config theo IP: memory trước, rồi KeyDB. Gọi khi DB get_by_ip lỗi.
pub async fn get_connection_server_by_ip(ip: &str) -> Option<TcocConnectionServer> {
    if let Some(e) = server_memory().get(ip) {
        return Some(e.clone());
    }
    if let Some(cache) = CACHE.get() {
        let key = format!("{}:{}", CachePrefix::ConnectionServer.as_str(), ip);
        if let Some(e) = cache.get::<TcocConnectionServer>(&key).await {
            server_memory().insert(ip.to_string(), e.clone());
            return Some(e);
        }
    }
    None
}

/// Lấy user theo username và toll_id: memory trước, rồi KeyDB. Gọi khi DB get_by_username_and_toll_id lỗi.
pub async fn get_connection_user(username: &str, toll_id: i64) -> Option<TcocUser> {
    let key = user_cache_key(username, toll_id);
    if let Some(u) = user_memory().get(&key) {
        return Some(u.clone());
    }
    if let Some(cache) = CACHE.get() {
        let full_key = format!("{}:{}", CachePrefix::ConnectionUser.as_str(), key);
        if let Some(u) = cache.get::<TcocUser>(&full_key).await {
            user_memory().insert(key.clone(), u.clone());
            return Some(u);
        }
    }
    None
}

/// Cập nhật cache (memory + KeyDB) khi DB get/save thành công — server theo IP.
pub async fn set_connection_server(ip: &str, entity: &TcocConnectionServer) {
    server_memory().insert(ip.to_string(), entity.clone());
    if let Some(cache) = CACHE.get() {
        let key = format!("{}:{}", CachePrefix::ConnectionServer.as_str(), ip);
        cache.set(&key, entity).await;
    }
}

/// Cập nhật cache (memory + KeyDB) khi DB get/save thành công — user theo username:toll_id.
pub async fn set_connection_user(username: &str, toll_id: i64, entity: &TcocUser) {
    let key = user_cache_key(username, toll_id);
    user_memory().insert(key.clone(), entity.clone());
    if let Some(cache) = CACHE.get() {
        let full_key = format!("{}:{}", CachePrefix::ConnectionUser.as_str(), key);
        cache.set(&full_key, entity).await;
    }
}
