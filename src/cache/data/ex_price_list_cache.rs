//! Cache Ex price list theo etag — logic giống wb_route_cache / connection_data_cache.
//! In-memory layer uses DashMap for concurrent get/set when cache is large.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::data::dto::ex_price_list_dto::ExPriceListItemDto;

/// Bộ nhớ in-process: etag -> Vec<ExPriceListItemDto>. DashMap for concurrent access.
type ExPriceByEtag = DashMap<String, Vec<ExPriceListItemDto>>;
static EX_PRICE_LIST_MEMORY: OnceLock<ExPriceByEtag> = OnceLock::new();

/// CacheManager cho KeyDB khi get_ex_price_list_by_etag (fallback) và set_ex_price_list.
static CACHE: OnceLock<Arc<CacheManager>> = OnceLock::new();

fn ex_price_list_memory() -> &'static ExPriceByEtag {
    EX_PRICE_LIST_MEMORY.get_or_init(DashMap::new)
}

/// Đăng ký CacheManager. Gọi từ get_toll_fee_list_cache khi load/reload.
pub fn set_cache(cache: Arc<CacheManager>) {
    if CACHE.get().is_none() {
        let _ = CACHE.set(cache);
    }
}

/// Điền memory từ dữ liệu reload (DB hoặc KeyDB fallback). Gọi sau atomic_reload_prefix hoặc load_prefix_from_keydb.
pub fn fill_memory_from_reload(by_etag: HashMap<String, Vec<ExPriceListItemDto>>) {
    let mem = ex_price_list_memory();
    mem.clear();
    for (etag, list) in by_etag {
        mem.insert(etag, list);
    }
}

/// Lấy Ex price list theo etag: memory trước, rồi KeyDB. Giống get_wb_route_by_etag.
pub async fn get_ex_price_list_by_etag(etag: &str) -> Option<Vec<ExPriceListItemDto>> {
    let etag = etag.trim();
    if etag.is_empty() {
        return None;
    }
    if let Some(list) = ex_price_list_memory().get(etag) {
        return Some(list.clone());
    }
    None
}
