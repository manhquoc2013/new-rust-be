//! Serialize FE_COMMIT_IN_RESP, lấy rating_detail từ TCD (BECT transport) cho commit.

use crate::cache::config::cache_manager::CacheManager;
use crate::cache::config::cache_prefix::CachePrefix;
use crate::constants::commit;
use crate::crypto::BlockEncryptMut;
use crate::crypto::Pkcs7;
use crate::db::repositories::TransportTransStageTcdRepository;
use crate::fe_protocol;
use crate::models::TCOCmessages::FE_COMMIT_IN_RESP;
use crate::models::VDTCmessages::RatingDetail as VDTCRatingDetail;
use crate::models::ETDR::clear_etdr_after_transaction_complete;
use crate::models::ETDR::BOORatingDetail;
use crate::price_ticket_type::from_db_string;
use crate::services::service::Service;
use aes;
use cbc;
use once_cell::sync::Lazy;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;

use crate::utils::timestamp_ms;

/// Serialize and encrypt FE_COMMIT_IN_RESP response.
pub(crate) async fn serialize_and_encrypt_commit_response(
    fe_resp: &FE_COMMIT_IN_RESP,
    encryptor: &cbc::Encryptor<aes::Aes128>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    if tracing::enabled!(tracing::Level::DEBUG) {
        tracing::debug!(
            request_id = fe_resp.request_id,
            session_id = fe_resp.session_id,
            command_id = fe_resp.command_id,
            status = fe_resp.status,
            "[FE] COMMIT_RESP returning to client"
        );
    }
    let cap = fe_protocol::response_header_status_len() as usize;
    let mut buffer_write = Vec::with_capacity(cap);
    buffer_write.write_i32_le(fe_resp.message_length).await?;
    buffer_write.write_i32_le(fe_resp.command_id).await?;
    fe_protocol::write_fe_request_id_session_id(
        &mut buffer_write,
        fe_resp.request_id,
        fe_resp.session_id,
    )
    .await?;
    buffer_write.write_i32_le(fe_resp.status).await?;

    let encrypted_reply = encryptor
        .clone()
        .encrypt_padded_vec_mut::<Pkcs7>(&buffer_write);
    Ok(crate::utils::wrap_encrypted_reply(encrypted_reply))
}

/// Map các field chung từ TCD (BOO_TRANS_STAGE_TCD / TRANSPORT_TRANS_STAGE_TCD) sang BOORatingDetail.
fn tcd_to_boo_rating_detail(
    boo: i64,
    toll_a_id: Option<i64>,
    toll_b_id: Option<i64>,
    ticket_type: Option<String>,
    price_ticket_type: Option<String>,
    price_amount: Option<i64>,
    subscription_id: Option<String>,
    vehicle_type: Option<i64>,
    price_id: Option<i64>,
    bot_id: Option<i64>,
    stage_id: Option<i64>,
) -> BOORatingDetail {
    BOORatingDetail {
        boo: boo as i32,
        toll_a_id: toll_a_id.unwrap_or(0) as i32,
        toll_b_id: toll_b_id.unwrap_or(0) as i32,
        ticket_type: ticket_type.unwrap_or_else(|| "L".to_string()),
        subscription_id,
        price_ticket_type: from_db_string(price_ticket_type.as_deref()),
        price_amount: price_amount.unwrap_or(0) as i32,
        vehicle_type: vehicle_type.unwrap_or(1) as i32,
        price_id,
        bot_id,
        stage_id,
    }
}

/// Lấy rating_detail từ DB (TRANSPORT_TRANS_STAGE_TCD) theo transport_trans_id.
/// Fallback khi ETDR cache không có rating_details hoặc rỗng.
pub(crate) async fn get_rating_detail_from_db(transport_trans_id: i64) -> Vec<BOORatingDetail> {
    let repo = TransportTransStageTcdRepository::new();
    match tokio::task::spawn_blocking(move || repo.find_by_transport_trans_id(transport_trans_id))
        .await
    {
        Ok(Ok(tcd_records)) => tcd_records
            .into_iter()
            .map(|tcd| {
                tcd_to_boo_rating_detail(
                    tcd.boo,
                    tcd.toll_a_id,
                    tcd.toll_b_id,
                    tcd.ticket_type,
                    tcd.price_ticket_type,
                    tcd.price_amount,
                    tcd.subscription_id,
                    tcd.vehicle_type,
                    tcd.price_id,
                    tcd.bot_id,
                    tcd.stage_id,
                )
            })
            .collect(),
        Ok(Err(e)) => {
            tracing::warn!(transport_trans_id, error = %e, "[DB] get rating_detail failed (TCD), using empty");
            Vec::new()
        }
        Err(e) => {
            tracing::warn!(transport_trans_id, error = %e, "[DB] get rating_detail task spawn failed, using empty");
            Vec::new()
        }
    }
}

/// Lấy rating_detail từ DB (TRANSPORT_TRANS_STAGE_TCD) theo transport_trans_id cho BECT.
/// Fallback khi ETDR cache không có rating_details hoặc rỗng.
pub(crate) async fn get_rating_detail_from_db_bect(
    transport_trans_id: i64,
) -> Vec<BOORatingDetail> {
    let repo = TransportTransStageTcdRepository::new();
    match tokio::task::spawn_blocking(move || repo.find_by_transport_trans_id(transport_trans_id))
        .await
    {
        Ok(Ok(tcd_records)) => tcd_records
            .into_iter()
            .map(|tcd| {
                tcd_to_boo_rating_detail(
                    tcd.boo,
                    tcd.toll_a_id,
                    tcd.toll_b_id,
                    tcd.ticket_type,
                    tcd.price_ticket_type,
                    tcd.price_amount,
                    tcd.subscription_id,
                    tcd.vehicle_type,
                    tcd.price_id,
                    tcd.bot_id,
                    tcd.stage_id,
                )
            })
            .collect(),
        Ok(Err(e)) => {
            tracing::warn!(transport_trans_id, error = %e, "[DB] get rating_detail BECT failed, using empty");
            Vec::new()
        }
        Err(e) => {
            tracing::warn!(transport_trans_id, error = %e, "[DB] get rating_detail BECT task spawn failed, using empty");
            Vec::new()
        }
    }
}

/// Chuyển VDTC/BECT RatingDetail sang BOORatingDetail (dùng khi ghi cache TCD từ checkin).
pub(crate) fn vdtc_rating_details_to_boo(details: &[VDTCRatingDetail]) -> Vec<BOORatingDetail> {
    details
        .iter()
        .map(|d| BOORatingDetail {
            boo: d.boo,
            toll_a_id: d.toll_a_id,
            toll_b_id: d.toll_b_id,
            ticket_type: d.ticket_type.trim_end_matches('\0').to_string(),
            subscription_id: if d.subscription_id.trim().is_empty() {
                None
            } else {
                Some(d.subscription_id.trim_end_matches('\0').to_string())
            },
            price_ticket_type: d.price_ticket_type,
            price_amount: d.price_amount,
            vehicle_type: d.vehicle_type,
            price_id: d.price_id,
            bot_id: d.bot_id,
            stage_id: d.stage_id,
        })
        .collect()
}

/// Ghi rating_detail vào cache TCD (sau khi checkin lưu TCD thành công).
pub(crate) async fn set_tcd_rating_cache(
    cache: &CacheManager,
    transport_trans_id: i64,
    is_bect: bool,
    details: &[BOORatingDetail],
) {
    let prefix = if is_bect {
        CachePrefix::TcdRatingBect
    } else {
        CachePrefix::TcdRatingBoo
    };
    let key = cache.gen_key(prefix, &[&transport_trans_id]);
    cache.set(&key, &details.to_vec()).await;
    tracing::debug!(
        transport_trans_id,
        is_bect,
        count = details.len(),
        "[Cache] TCD rating_detail set"
    );
}

/// Xóa cache TCD theo transport_trans_id (gọi khi clear ETDR sau giao dịch hoàn tất để tránh dữ liệu cũ).
pub(crate) async fn invalidate_tcd_rating_cache(
    cache: &CacheManager,
    transport_trans_id: i64,
    is_bect: bool,
) {
    let prefix = if is_bect {
        CachePrefix::TcdRatingBect
    } else {
        CachePrefix::TcdRatingBoo
    };
    let key = cache.gen_key(prefix, &[&transport_trans_id]);
    cache.remove(&key).await;
    tracing::debug!(
        transport_trans_id,
        is_bect,
        "[Cache] TCD rating_detail invalidated"
    );
}

#[allow(dead_code)]
static ETDR_CLEANUP_SEMAPHORE: Lazy<Arc<Semaphore>> =
    Lazy::new(|| Arc::new(Semaphore::new(commit::MAX_CONCURRENT_ETDR_CLEANUP)));

#[allow(dead_code)]
/// Tham số cho cleanup ETDR sau checkout commit (chạy background, giới hạn đồng thời).
pub(crate) struct EtdrCleanupAfterCheckoutParams {
    pub etag_norm: String,
    pub transport_trans_id: i64,
    pub cache: Arc<CacheManager>,
    pub request_id: i64,
    pub conn_id: i32,
    pub log_prefix: &'static str,
    pub etag_display: String,
    pub actual_command_id: Option<i32>,
    /// Checkout datetime (ms since epoch) from BOO record; if DB verify fails but now - this > 5min, allow clear.
    pub checkout_datetime_ms: Option<i64>,
}

#[allow(dead_code)]
/// Spawn task cleanup ETDR + invalidate TCD sau checkout commit. Giới hạn đồng thời bằng semaphore để tránh ảnh hưởng perf khi nhiều request (vd. 50 task).
/// Chỉ clear cache sau khi verify từ DB (với retries) rằng BOO record đã có checkout_commit_datetime, tránh xoá cache khi DB lưu thất bại hoặc chưa visible.
pub(crate) fn spawn_etdr_cleanup_after_checkout(params: EtdrCleanupAfterCheckoutParams) {
    let semaphore = ETDR_CLEANUP_SEMAPHORE.clone();
    tokio::spawn(async move {
        let _permit = match semaphore.acquire().await {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!(
                    conn_id = params.conn_id,
                    request_id = params.request_id,
                    transport_trans_id = params.transport_trans_id,
                    "[{}] ETDR cleanup semaphore closed, skipping",
                    params.log_prefix
                );
                return;
            }
        };
        let start = Instant::now();
        tracing::info!(
            conn_id = params.conn_id,
            request_id = params.request_id,
            transport_trans_id = params.transport_trans_id,
            etag = %params.etag_display.trim(),
            actual_command_id = ?params.actual_command_id,
            "{} cleanup after checkout commit started (background)",
            params.log_prefix
        );

        // Verify from DB with retries before clearing: only clear when TRANSPORT_TRANSACTION_STAGE has checkout_commit_datetime persisted.
        let transport_service = crate::services::TransportTransactionStageService::default();
        let mut verified = false;
        for attempt in 0..=commit::VERIFY_BEFORE_CLEANUP_MAX_RETRIES {
            match transport_service.get_by_id(params.transport_trans_id).await {
                Ok(Some(ref record)) if record.checkout_commit_datetime.is_some() => {
                    verified = true;
                    if attempt > 0 {
                        tracing::debug!(
                            conn_id = params.conn_id,
                            request_id = params.request_id,
                            transport_trans_id = params.transport_trans_id,
                            attempt,
                            "{} TRANSPORT_TRANSACTION_STAGE verified from DB before cleanup (after retry)",
                            params.log_prefix
                        );
                    }
                    break;
                }
                Ok(Some(_)) => {
                    if attempt < commit::VERIFY_BEFORE_CLEANUP_MAX_RETRIES {
                        let delay_ms =
                            commit::VERIFY_BEFORE_CLEANUP_DELAY_MS * (attempt as u64 + 1);
                        tracing::debug!(
                            conn_id = params.conn_id,
                            request_id = params.request_id,
                            transport_trans_id = params.transport_trans_id,
                            attempt = attempt + 1,
                            delay_ms,
                            "{} record CHECKOUT_COMMIT_DATETIME not yet set, retrying before clear",
                            params.log_prefix
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                    }
                }
                Ok(None) | Err(_) => {
                    if attempt < commit::VERIFY_BEFORE_CLEANUP_MAX_RETRIES {
                        let delay_ms =
                            commit::VERIFY_BEFORE_CLEANUP_DELAY_MS * (attempt as u64 + 1);
                        tracing::debug!(
                            conn_id = params.conn_id,
                            request_id = params.request_id,
                            transport_trans_id = params.transport_trans_id,
                            attempt = attempt + 1,
                            delay_ms,
                            "{} record not found from DB, retrying before clear",
                            params.log_prefix
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        if !verified {
            // Allow clear if >5min since checkout_datetime (avoid keeping stale ETDR forever when DB verify fails).
            let allow_clear_after_ms = commit::ALLOW_CLEAR_AFTER_CHECKOUT_MS as i64;
            if let Some(ts) = params.checkout_datetime_ms {
                let elapsed = timestamp_ms() - ts;
                if elapsed >= allow_clear_after_ms {
                    verified = true;
                    tracing::info!(
                        conn_id = params.conn_id,
                        request_id = params.request_id,
                        transport_trans_id = params.transport_trans_id,
                        etag = %params.etag_display.trim(),
                        elapsed_ms = elapsed,
                        threshold_ms = allow_clear_after_ms,
                        "{} allow ETDR cleanup: >5min since checkout_datetime (DB verify failed after retries)",
                        params.log_prefix
                    );
                }
            }
        }

        if !verified {
            tracing::warn!(
                conn_id = params.conn_id,
                request_id = params.request_id,
                transport_trans_id = params.transport_trans_id,
                etag = %params.etag_display.trim(),
                retries = commit::VERIFY_BEFORE_CLEANUP_MAX_RETRIES,
                "{} skip ETDR cleanup: DB verify failed after retries (record not found or CHECKOUT_COMMIT_DATETIME null)",
                params.log_prefix
            );
            return;
        }

        clear_etdr_after_transaction_complete(&params.etag_norm, Some(params.transport_trans_id))
            .await;
        invalidate_tcd_rating_cache(params.cache.as_ref(), params.transport_trans_id, false).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        tracing::info!(
            conn_id = params.conn_id,
            request_id = params.request_id,
            transport_trans_id = params.transport_trans_id,
            etag = %params.etag_display.trim(),
            elapsed_ms,
            actual_command_id = ?params.actual_command_id,
            "{} cleanup after checkout commit finished (ETDR cleared, TCD cache invalidated)",
            params.log_prefix
        );
    });
}

/// Lấy rating_detail: ưu tiên cache, không có thì load from DB rồi ghi cache.
pub(crate) async fn get_rating_detail_cached(
    cache: &CacheManager,
    transport_trans_id: i64,
    is_bect: bool,
) -> Vec<BOORatingDetail> {
    let prefix = if is_bect {
        CachePrefix::TcdRatingBect
    } else {
        CachePrefix::TcdRatingBoo
    };
    let key = cache.gen_key(prefix, &[&transport_trans_id]);
    if let Some(cached) = cache.get::<Vec<BOORatingDetail>>(&key).await {
        tracing::debug!(
            transport_trans_id,
            is_bect,
            count = cached.len(),
            "[Cache] TCD rating_detail hit"
        );
        return cached;
    }
    let details = if is_bect {
        get_rating_detail_from_db_bect(transport_trans_id).await
    } else {
        get_rating_detail_from_db(transport_trans_id).await
    };
    cache.set(&key, &details).await;
    if !details.is_empty() {
        tracing::debug!(
            transport_trans_id,
            is_bect,
            count = details.len(),
            "[Cache] TCD rating_detail loaded from DB and cached"
        );
    }
    details
}
