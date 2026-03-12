//! Hàng đợi ghi DB qua KeyDB: khi save/update DB lỗi thì ghi vào KeyDB, task định kỳ retry.
//! Insert và update dùng chung queue: một key theo bản ghi (conn_server theo ip, user theo username:toll_id), value có id: None = insert, Some(id) = update; ghi đè cùng key tránh duplicate.
//! Khi ghi KeyDB thất bại: ghi vào buffer in-memory (giới hạn kích thước), task retry sẽ flush buffer lên KeyDB khi KeyDB có lại.
//! Dùng mediation_pool_holder (cùng nguồn với cache_reload) để tránh conflict khi MEDIATION init lỗi rồi reconnect.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use crate::cache::config::cache_prefix::CachePrefix;
use crate::cache::config::keydb::KeyDB;
use crate::configs::pool_factory::OdbcConnectionManager;
use crate::constants::db_retry;
use crate::db::repositories::{TcocConnectionServer, TcocSession, TcocUser};
use crate::db::repositories::{
    TcocConnectionServerRepository, TcocSessionRepository, TcocUserRepository,
};
use crate::db::Repository;
use r2d2::Pool;

/// Số bản ghi tối đa trong buffer fallback cho session (FIFO drop khi đầy).
/// Số bản ghi tối đa trong buffer fallback cho conn_server/user (theo logical key, ghi đè bản mới nhất).
/// Số thao tác ghi DB retry chạy song song mỗi loại (session, conn_server, user) để giảm delay.
/// Kích thước batch khi flush buffer lên KeyDB (giảm round-trip).
/// Buffer in-memory khi enqueue KeyDB thất bại; task retry flush lên KeyDB khi KeyDB connected.
struct DbRetryBuffer {
    sessions: Vec<TcocSession>,
    /// Key = ip
    conn_servers: HashMap<String, DbRetryConnServerEntry>,
    /// Key = username:toll_id
    users: HashMap<String, DbRetryTcocUserEntry>,
}

/// Entry cho conn_server: id None = insert, Some(id) = update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbRetryConnServerEntry {
    pub id: Option<i64>,
    pub entity: TcocConnectionServer,
}

/// Entry cho user: id None = insert, Some(id) = update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbRetryTcocUserEntry {
    pub id: Option<i64>,
    pub entity: TcocUser,
}

impl Default for DbRetryBuffer {
    fn default() -> Self {
        Self {
            sessions: Vec::with_capacity(db_retry::BUFFER_MAX_SESSIONS.min(64)),
            conn_servers: HashMap::new(),
            users: HashMap::new(),
        }
    }
}

static KEYDB: std::sync::OnceLock<Arc<KeyDB>> = std::sync::OnceLock::new();
static BUFFER: std::sync::OnceLock<tokio::sync::Mutex<DbRetryBuffer>> = std::sync::OnceLock::new();

fn get_buffer() -> &'static tokio::sync::Mutex<DbRetryBuffer> {
    BUFFER.get_or_init(|| tokio::sync::Mutex::new(DbRetryBuffer::default()))
}

/// Đăng ký KeyDB cho db_retry. Gọi từ main sau khi tạo cache.
pub fn set_keydb(keydb: Arc<KeyDB>) {
    let _ = KEYDB.set(keydb);
}

fn keydb() -> Option<Arc<KeyDB>> {
    KEYDB.get().cloned()
}

/// Lưu session vào KeyDB để retry insert. Key = prefix:session_id (một key = một bản ghi mới nhất).
/// Returns true if written vào KeyDB; false khi KeyDB không kết nối hoặc ghi lỗi. Khi false sẽ ghi vào buffer in-memory để task retry flush khi KeyDB có lại.
pub async fn enqueue_tcoc_session(session: &TcocSession) -> bool {
    if let Some(k) = keydb() {
        let key = format!(
            "{}:{}",
            CachePrefix::DbRetryTcocSession.as_str(),
            session.session_id
        );
        match k.set_result(&key, session).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(
                    session_id = session.session_id,
                    error = %e,
                    "[DB] db_retry enqueue TcocSession to KeyDB failed, pushing to buffer"
                );
                let mut buf = get_buffer().lock().await;
                if buf.sessions.len() >= db_retry::BUFFER_MAX_SESSIONS {
                    buf.sessions.remove(0);
                }
                buf.sessions.push(session.clone());
                false
            }
        }
    } else {
        tracing::debug!("[DB] db_retry KeyDB not set, enqueue TcocSession skipped");
        false
    }
}

/// Lưu connection server vào KeyDB để retry insert/update. Key = prefix:ip (một key một bản ghi); id None = insert, Some(id) = update; ghi đè cùng key tránh duplicate.
/// Returns true if written vào KeyDB; false khi KeyDB không kết nối hoặc ghi lỗi. Khi false ghi vào buffer để task retry flush.
pub async fn enqueue_tcoc_conn_server(
    option_id: Option<i64>,
    entity: &TcocConnectionServer,
) -> bool {
    let logical_key = entity.ip.clone();
    let entry = DbRetryConnServerEntry {
        id: option_id,
        entity: entity.clone(),
    };
    if let Some(k) = keydb() {
        let key = format!(
            "{}:{}",
            CachePrefix::DbRetryConnServer.as_str(),
            logical_key
        );
        match k.set_result(&key, &entry).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(
                    ip = %logical_key,
                    error = %e,
                    "[DB] db_retry enqueue TcocConnectionServer to KeyDB failed, pushing to buffer"
                );
                let mut buf = get_buffer().lock().await;
                if !buf.conn_servers.contains_key(&logical_key)
                    && buf.conn_servers.len() >= db_retry::BUFFER_MAX_BY_KEY
                {
                    let first_key = buf.conn_servers.keys().next().cloned().unwrap_or_default();
                    buf.conn_servers.remove(&first_key);
                }
                buf.conn_servers.insert(logical_key, entry);
                false
            }
        }
    } else {
        tracing::debug!("[DB] db_retry KeyDB not set, enqueue TcocConnectionServer skipped");
        false
    }
}

fn user_logical_key(entity: &TcocUser) -> String {
    format!("{}:{}", entity.user_name, entity.toll_id.unwrap_or(0))
}

/// Lưu user vào KeyDB để retry insert/update. Key = prefix:username:toll_id (một key một bản ghi); id None = insert, Some(id) = update; ghi đè cùng key tránh duplicate.
/// Returns true if written vào KeyDB; false khi KeyDB không kết nối hoặc ghi lỗi. Khi false ghi vào buffer để task retry flush.
pub async fn enqueue_tcoc_user(option_id: Option<i64>, entity: &TcocUser) -> bool {
    let logical_key = user_logical_key(entity);
    let entry = DbRetryTcocUserEntry {
        id: option_id,
        entity: entity.clone(),
    };
    if let Some(k) = keydb() {
        let key = format!("{}:{}", CachePrefix::DbRetryTcocUser.as_str(), logical_key);
        match k.set_result(&key, &entry).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(
                    key = %logical_key,
                    error = %e,
                    "[DB] db_retry enqueue TcocUser to KeyDB failed, pushing to buffer"
                );
                let mut buf = get_buffer().lock().await;
                if !buf.users.contains_key(&logical_key)
                    && buf.users.len() >= db_retry::BUFFER_MAX_BY_KEY
                {
                    let first_key = buf.users.keys().next().cloned().unwrap_or_default();
                    buf.users.remove(&first_key);
                }
                buf.users.insert(logical_key, entry);
                false
            }
        }
    } else {
        tracing::debug!("[DB] db_retry KeyDB not set, enqueue TcocUser skipped");
        false
    }
}

/// Task định kỳ: đọc KeyDB theo từng prefix, thử ghi DB, xóa key khi thành công.
/// Mỗi chu kỳ: flush buffer in-memory lên KeyDB (nếu KeyDB connected), rồi xử lý keys từ KeyDB như cũ.
/// Dùng mediation_pool_holder (cùng nguồn với cache_reload) để tránh hai pool MEDIATION khi init lỗi rồi reconnect.
pub async fn run_db_retry_task(
    interval_secs: u64,
    mediation_pool_holder: Arc<std::sync::RwLock<Option<Arc<Pool<OdbcConnectionManager>>>>>,
) {
    let k = match keydb() {
        Some(k) => k,
        None => {
            tracing::debug!("[DB] db_retry KeyDB not set, retry task skipped");
            return;
        }
    };

    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
    loop {
        interval.tick().await;

        if k.is_connected().await {
            let to_flush = {
                let mut buf = get_buffer().lock().await;
                let sessions: Vec<TcocSession> = std::mem::take(&mut buf.sessions);
                let conn_servers: Vec<(String, DbRetryConnServerEntry)> =
                    buf.conn_servers.drain().collect();
                let users: Vec<(String, DbRetryTcocUserEntry)> = buf.users.drain().collect();
                (sessions, conn_servers, users)
            };
            let (sessions, conn_servers, users) = to_flush;
            let mut flushed_session = 0u32;
            let mut flushed_conn = 0u32;
            let mut flushed_user = 0u32;

            // Flush sessions: batch set_many_result để giảm round-trip KeyDB
            let prefix_session = CachePrefix::DbRetryTcocSession.as_str();
            for chunk in sessions.chunks(db_retry::FLUSH_BATCH_SIZE) {
                let pairs: Vec<(String, TcocSession)> = chunk
                    .iter()
                    .map(|s| (format!("{}:{}", prefix_session, s.session_id), s.clone()))
                    .collect();
                match k.set_many_result(&pairs).await {
                    Ok(()) => flushed_session += chunk.len() as u32,
                    Err(_) => {
                        let mut buf = get_buffer().lock().await;
                        for s in chunk {
                            if buf.sessions.len() < db_retry::BUFFER_MAX_SESSIONS {
                                buf.sessions.push(s.clone());
                            }
                        }
                    }
                }
            }
            // Flush conn_servers: batch set_many_result
            let prefix_conn = CachePrefix::DbRetryConnServer.as_str();
            for chunk in conn_servers.chunks(db_retry::FLUSH_BATCH_SIZE) {
                let pairs: Vec<(String, DbRetryConnServerEntry)> = chunk
                    .iter()
                    .map(|(logical_key, entry)| {
                        (format!("{}:{}", prefix_conn, logical_key), entry.clone())
                    })
                    .collect();
                match k.set_many_result(&pairs).await {
                    Ok(()) => flushed_conn += chunk.len() as u32,
                    Err(_) => {
                        let mut buf = get_buffer().lock().await;
                        for (k, v) in chunk {
                            if buf.conn_servers.len() < db_retry::BUFFER_MAX_BY_KEY
                                || buf.conn_servers.contains_key(k)
                            {
                                buf.conn_servers.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
            }
            // Flush users: batch set_many_result
            let prefix_user = CachePrefix::DbRetryTcocUser.as_str();
            for chunk in users.chunks(db_retry::FLUSH_BATCH_SIZE) {
                let pairs: Vec<(String, DbRetryTcocUserEntry)> = chunk
                    .iter()
                    .map(|(logical_key, entry)| {
                        (format!("{}:{}", prefix_user, logical_key), entry.clone())
                    })
                    .collect();
                match k.set_many_result(&pairs).await {
                    Ok(()) => flushed_user += chunk.len() as u32,
                    Err(_) => {
                        let mut buf = get_buffer().lock().await;
                        for (k, v) in chunk {
                            if buf.users.len() < db_retry::BUFFER_MAX_BY_KEY
                                || buf.users.contains_key(k)
                            {
                                buf.users.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
            }

            if flushed_session > 0 || flushed_conn > 0 || flushed_user > 0 {
                tracing::info!(
                    sessions = flushed_session,
                    conn_servers = flushed_conn,
                    users = flushed_user,
                    "[DB] db_retry buffer flushed to KeyDB"
                );
            }
        }

        let mediation_pool = mediation_pool_holder
            .read()
            .ok()
            .and_then(|g| g.as_ref().cloned());
        if mediation_pool.is_none() {
            tracing::debug!("[DB] db_retry MEDIATION pool not available, skip retry this cycle");
        }

        if let Some(ref pool) = mediation_pool {
            let sem = Arc::new(Semaphore::new(db_retry::RETRY_CONCURRENCY));
            let prefix_session = format!("{}:", CachePrefix::DbRetryTcocSession.as_str());
            if let Ok(keys) = k.get_keys_with_prefix(&prefix_session).await {
                let mut handles = Vec::with_capacity(keys.len());
                for key in keys {
                    let k_clone = k.clone();
                    let pool_clone = pool.clone();
                    let sem_clone = sem.clone();
                    let prefix_session = prefix_session.clone();
                    handles.push(tokio::spawn(async move {
                        let _permit = match sem_clone.acquire().await {
                            Ok(p) => p,
                            Err(_) => return,
                        };
                        let session_id_str = key.strip_prefix(&prefix_session).unwrap_or("");
                        let session_id: i64 = match session_id_str.parse() {
                            Ok(x) => x,
                            Err(_) => return,
                        };
                        let session = match k_clone.get::<TcocSession>(&key).await {
                            Some(s) => s,
                            None => return,
                        };
                        let result = tokio::task::spawn_blocking({
                            let pool_inner = pool_clone.clone();
                            let session_inner = session.clone();
                            move || {
                                let repo = TcocSessionRepository::with_pool(pool_inner.as_ref());
                                repo.insert(&session_inner)
                            }
                        })
                        .await;
                        match result {
                            Ok(Ok(_)) => {
                                let _ = k_clone.remove(&key).await;
                                tracing::info!(session_id, "[DB] db_retry TcocSession insert ok");
                            }
                            Ok(Err(e)) => {
                                tracing::warn!(session_id, error = %e, "[DB] db_retry TcocSession insert failed");
                            }
                            Err(e) => {
                                tracing::warn!(session_id, error = %e, "[DB] db_retry TcocSession task join failed");
                            }
                        }
                    }));
                }
                for h in handles {
                    let _ = h.await;
                }
            }

            let prefix_conn = format!("{}:", CachePrefix::DbRetryConnServer.as_str());
            if let Ok(keys) = k.get_keys_with_prefix(&prefix_conn).await {
                let sem_conn = Arc::new(Semaphore::new(db_retry::RETRY_CONCURRENCY));
                let mut handles = Vec::with_capacity(keys.len());
                for key in keys {
                    let k_clone = k.clone();
                    let pool_clone = pool.clone();
                    let sem_clone = sem_conn.clone();
                    let prefix_conn = prefix_conn.clone();
                    handles.push(tokio::spawn(async move {
                        let _permit = match sem_clone.acquire().await {
                            Ok(p) => p,
                            Err(_) => return,
                        };
                        let suffix = key.strip_prefix(&prefix_conn).unwrap_or("");
                        if let Some(entry) = k_clone.get::<DbRetryConnServerEntry>(&key).await {
                            let ent = entry.entity.clone();
                            let (is_insert, result) = match entry.id {
                                None => (
                                    true,
                                    tokio::task::spawn_blocking({
                                        let pool_inner = pool_clone.clone();
                                        let ent_inner = ent.clone();
                                        move || {
                                            let repo = TcocConnectionServerRepository::with_pool(pool_inner.as_ref());
                                            repo.insert(&ent_inner)
                                        }
                                    })
                                    .await
                                    .map(|r| r.map(|_| true)),
                                ),
                                Some(id) => (
                                    false,
                                    tokio::task::spawn_blocking({
                                        let pool_inner = pool_clone.clone();
                                        let ent_inner = ent.clone();
                                        move || {
                                            let repo = TcocConnectionServerRepository::with_pool(pool_inner.as_ref());
                                            repo.update(id, &ent_inner)
                                        }
                                    })
                                    .await,
                                ),
                            };
                            match result {
                                Ok(Ok(success)) => {
                                    if success {
                                        let _ = k_clone.remove(&key).await;
                                        if is_insert {
                                            tracing::info!(ip = %suffix, "[DB] db_retry TcocConnectionServer insert ok");
                                        } else {
                                            tracing::info!(ip = %suffix, "[DB] db_retry TcocConnectionServer update ok");
                                        }
                                    } else if !is_insert {
                                        tracing::warn!(ip = %suffix, "[DB] db_retry TcocConnectionServer update returned false");
                                    }
                                }
                                Ok(Err(e)) => {
                                    if is_insert {
                                        tracing::warn!(ip = %suffix, error = %e, "[DB] db_retry TcocConnectionServer insert failed");
                                    } else {
                                        tracing::warn!(ip = %suffix, error = %e, "[DB] db_retry TcocConnectionServer update failed");
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(ip = %suffix, error = %e, "[DB] db_retry TcocConnectionServer task join failed");
                                }
                            }
                            return;
                        }
                        if let Some(entity) = k_clone.get::<TcocConnectionServer>(&key).await {
                            if let Ok(id) = suffix.parse::<i64>() {
                                let pool_inner = pool_clone.clone();
                                let ent = entity.clone();
                                let result = tokio::task::spawn_blocking(move || {
                                    let repo =
                                        TcocConnectionServerRepository::with_pool(pool_inner.as_ref());
                                    repo.update(id, &ent)
                                })
                                .await;
                                match result {
                                    Ok(Ok(true)) => {
                                        let _ = k_clone.remove(&key).await;
                                        tracing::info!(id, "[DB] db_retry TcocConnectionServer update ok (legacy key)");
                                    }
                                    Ok(Ok(false)) => {
                                        tracing::warn!(id, "[DB] db_retry TcocConnectionServer update returned false");
                                    }
                                    Ok(Err(e)) => {
                                        tracing::warn!(id, error = %e, "[DB] db_retry TcocConnectionServer update failed");
                                    }
                                    Err(e) => {
                                        tracing::warn!(id, error = %e, "[DB] db_retry TcocConnectionServer task join failed");
                                    }
                                }
                            }
                        }
                    }));
                }
                for h in handles {
                    let _ = h.await;
                }
            }

            let prefix_user = format!("{}:", CachePrefix::DbRetryTcocUser.as_str());
            if let Ok(keys) = k.get_keys_with_prefix(&prefix_user).await {
                let sem_user = Arc::new(Semaphore::new(db_retry::RETRY_CONCURRENCY));
                let mut handles = Vec::with_capacity(keys.len());
                for key in keys {
                    let k_clone = k.clone();
                    let pool_clone = pool.clone();
                    let sem_clone = sem_user.clone();
                    let prefix_user = prefix_user.clone();
                    handles.push(tokio::spawn(async move {
                        let _permit = match sem_clone.acquire().await {
                            Ok(p) => p,
                            Err(_) => return,
                        };
                        let suffix = key.strip_prefix(&prefix_user).unwrap_or("");
                        if let Some(entry) = k_clone.get::<DbRetryTcocUserEntry>(&key).await {
                            let ent = entry.entity.clone();
                            let (is_insert, result) = match entry.id {
                                None => (
                                    true,
                                    tokio::task::spawn_blocking({
                                        let pool_inner = pool_clone.clone();
                                        let ent_inner = ent.clone();
                                        move || {
                                            let repo = TcocUserRepository::with_pool(pool_inner.as_ref());
                                            repo.insert(&ent_inner)
                                        }
                                    })
                                    .await
                                    .map(|r| r.map(|_| true)),
                                ),
                                Some(id) => (
                                    false,
                                    tokio::task::spawn_blocking({
                                        let pool_inner = pool_clone.clone();
                                        let ent_inner = ent.clone();
                                        move || {
                                            let repo = TcocUserRepository::with_pool(pool_inner.as_ref());
                                            repo.update(id, &ent_inner)
                                        }
                                    })
                                    .await,
                                ),
                            };
                            match result {
                                Ok(Ok(success)) => {
                                    if success {
                                        let _ = k_clone.remove(&key).await;
                                        if is_insert {
                                            tracing::info!(key = %suffix, "[DB] db_retry TcocUser insert ok");
                                        } else {
                                            tracing::info!(key = %suffix, "[DB] db_retry TcocUser update ok");
                                        }
                                    } else if !is_insert {
                                        tracing::warn!(key = %suffix, "[DB] db_retry TcocUser update returned false");
                                    }
                                }
                                Ok(Err(e)) => {
                                    if is_insert {
                                        tracing::warn!(key = %suffix, error = %e, "[DB] db_retry TcocUser insert failed");
                                    } else {
                                        tracing::warn!(key = %suffix, error = %e, "[DB] db_retry TcocUser update failed");
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(key = %suffix, error = %e, "[DB] db_retry TcocUser task join failed");
                                }
                            }
                            return;
                        }
                        if let Some(entity) = k_clone.get::<TcocUser>(&key).await {
                            if let Ok(id) = suffix.parse::<i64>() {
                                let pool_inner = pool_clone.clone();
                                let ent = entity.clone();
                                let result = tokio::task::spawn_blocking(move || {
                                    let repo = TcocUserRepository::with_pool(pool_inner.as_ref());
                                    repo.update(id, &ent)
                                })
                                .await;
                                match result {
                                    Ok(Ok(true)) => {
                                        let _ = k_clone.remove(&key).await;
                                        tracing::info!(id, "[DB] db_retry TcocUser update ok (legacy key)");
                                    }
                                    Ok(Ok(false)) => {
                                        tracing::warn!(id, "[DB] db_retry TcocUser update returned false");
                                    }
                                    Ok(Err(e)) => {
                                        tracing::warn!(id, error = %e, "[DB] db_retry TcocUser update failed");
                                    }
                                    Err(e) => {
                                        tracing::warn!(id, error = %e, "[DB] db_retry TcocUser task join failed");
                                    }
                                }
                            }
                        }
                    }));
                }
                for h in handles {
                    let _ = h.await;
                }
            }
        }
    }
}
