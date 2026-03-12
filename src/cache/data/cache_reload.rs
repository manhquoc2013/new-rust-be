//! Periodically reload all caches (toll, lane, price, stage, toll_fee_list, subscription_history, connection server/user), start_cache_reload_task.
//!
//! **Reload flow**: Prefer DB; khi DB thành công → ghi Moka + KeyDB (atomic_reload_prefix). Khi DB lỗi hoặc pool không có → fallback read from KeyDB (load_from_keydb = true) để hệ thống không gián đoạn khi DB lost.
//! Connection server/user only reload when mediation pool available (DB đã kết nối hoặc đã retry thành công).

use r2d2::Pool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::time::{interval, Duration};
use tracing::info;

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::data::closed_cycle_transition_stage_cache::get_closed_cycle_transition_stage_cache;
use crate::cache::data::connection_data_cache::{
    get_connection_server_cache, get_connection_user_cache,
};
use crate::cache::data::price_cache::get_price_cache;
use crate::cache::data::subscription_history_cache::get_subscription_history_cache;
use crate::cache::data::toll_cache::get_toll_cache;
use crate::cache::data::toll_fee_list_cache::get_toll_fee_list_cache;
use crate::cache::data::toll_lane_cache::get_toll_lane_cache;
use crate::configs::pool_factory::OdbcConnectionManager;

/// Tránh chạy nhiều lần reload đồng thời; nếu đang reload thì bỏ qua chu kỳ hiện tại.
static RELOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Khi drop (task kết thúc hoặc panic) luôn set RELOAD_IN_PROGRESS = false để chu kỳ sau chạy lại.
struct ReloadGuard;
impl Drop for ReloadGuard {
    fn drop(&mut self) {
        RELOAD_IN_PROGRESS.store(false, Ordering::Release);
    }
}

/// Reload tất cả cache: ưu tiên DB; DB thành công ghi Moka + KeyDB; DB lỗi/không có pool thì fallback KeyDB (load_from_keydb = true) để khi DB lost hệ thống vẫn chạy.
/// RATING cache chỉ reload khi rating_pool_holder có pool; connection server/user chỉ khi mediation_pool_holder có pool.
pub async fn reload_all_cache(
    rating_pool_holder: Arc<RwLock<Option<Arc<Pool<OdbcConnectionManager>>>>>,
    cache: Arc<CacheManager>,
    mediation_pool_holder: Arc<RwLock<Option<Arc<Pool<OdbcConnectionManager>>>>>,
) {
    let t_start = Instant::now();
    let rating_pool = rating_pool_holder
        .read()
        .ok()
        .and_then(|g| g.as_ref().cloned());
    let load_from_keydb = true;
    let ((), (), (), (), (), ()) = tokio::join!(
        get_toll_cache(rating_pool.clone(), cache.clone(), load_from_keydb),
        get_toll_lane_cache(rating_pool.clone(), cache.clone(), load_from_keydb),
        get_price_cache(rating_pool.clone(), cache.clone(), load_from_keydb),
        get_closed_cycle_transition_stage_cache(
            rating_pool.clone(),
            cache.clone(),
            load_from_keydb
        ),
        get_toll_fee_list_cache(rating_pool.clone(), cache.clone(), load_from_keydb),
        get_subscription_history_cache(rating_pool, cache.clone(), load_from_keydb),
    );
    let med_pool = mediation_pool_holder
        .read()
        .ok()
        .and_then(|g| g.as_ref().cloned());
    if let Some(med_pool) = med_pool {
        tokio::join!(
            get_connection_server_cache(Some(med_pool.clone()), cache.clone(), load_from_keydb),
            get_connection_user_cache(Some(med_pool), cache.clone(), load_from_keydb),
        );
    }
    let elapsed = t_start.elapsed();
    info!(
        elapsed_ms = elapsed.as_millis(),
        "[CacheReload] reload completed"
    );
}

/// Khởi động background task để reload cache định kỳ. RATING cache reload khi rating_pool_holder có pool; connection data khi mediation_pool_holder có pool.
pub fn start_cache_reload_task(
    rating_pool_holder: Arc<RwLock<Option<Arc<Pool<OdbcConnectionManager>>>>>,
    cache: Arc<CacheManager>,
    reload_interval_seconds: u64,
    mediation_pool_holder: Arc<RwLock<Option<Arc<Pool<OdbcConnectionManager>>>>>,
) {
    info!(
        interval_secs = reload_interval_seconds,
        "[CacheReload] Task started"
    );
    tokio::spawn(async move {
        let mut interval_timer = interval(Duration::from_secs(reload_interval_seconds));
        interval_timer.tick().await;
        loop {
            interval_timer.tick().await;
            if RELOAD_IN_PROGRESS
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                let rating = rating_pool_holder.clone();
                let c = cache.clone();
                let med = mediation_pool_holder.clone();
                tokio::spawn(async move {
                    let _guard = ReloadGuard;
                    reload_all_cache(rating, c, med).await;
                });
            } else {
                tracing::debug!("[CacheReload] skip cycle (reload in progress)");
            }
        }
    });
}
