//! Cache WB list (whitelist/blacklist route) theo etag — logic giống connection_data_cache.
//! In-memory layer uses DashMap for concurrent get/set when cache is large.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::data::dto::wb_list_route_dto::WbListRouteDto;

/// Bộ nhớ in-process: etag (đã normalize) -> Vec<WbListRouteDto>. DashMap for concurrent access.
type WbByEtag = DashMap<String, Vec<WbListRouteDto>>;
static WB_ROUTE_MEMORY: OnceLock<WbByEtag> = OnceLock::new();

/// CacheManager cho KeyDB khi get_wb_route_by_etag (fallback) và set_wb_route.
static CACHE: OnceLock<Arc<CacheManager>> = OnceLock::new();

fn wb_memory() -> &'static WbByEtag {
    WB_ROUTE_MEMORY.get_or_init(DashMap::new)
}

/// Đăng ký CacheManager. Gọi từ get_toll_fee_list_cache khi load/reload.
pub fn set_cache(cache: Arc<CacheManager>) {
    if CACHE.get().is_none() {
        let _ = CACHE.set(cache);
    }
}

/// Điền memory từ dữ liệu reload (DB hoặc KeyDB fallback). Gọi sau atomic_reload_prefix hoặc load_prefix_from_keydb.
pub fn fill_memory_from_reload(by_etag: HashMap<String, Vec<WbListRouteDto>>) {
    let mem = wb_memory();
    mem.clear();
    for (etag, list) in by_etag {
        mem.insert(etag, list);
    }
}

/// Lấy WB list theo etag: memory trước, rồi KeyDB. Giống get_connection_server_by_ip / get_connection_user.
pub async fn get_wb_route_by_etag(etag: &str) -> Option<Vec<WbListRouteDto>> {
    let etag = etag.trim();
    if etag.is_empty() {
        return None;
    }
    if let Some(list) = wb_memory().get(etag) {
        return Some(list.clone());
    }
    None
}
