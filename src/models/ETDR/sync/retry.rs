//! Retry save ETDR to DB: merge+update if record exists, build+save otherwise.
//! Idempotent for HA: get_by_id(ticket_id) first; repo insert skips when record exists (by ticket_in_id or transport_trans_id); duplicate key → get_by_id then treat as success.
//! Cross-node: skip insert when no record in DB but etdr has ticket_in_id (another node may have saved with a different ticket_id).
//!
//! **Stage (transaction) vs TCD:** Stage save/update does not use a shared lock per transport_trans_id. Insert is idempotent at repo (ticket_in_id, transport_trans_id); update may run concurrently from checkin, commit handler and retry task (last write wins; merge combines fields). Only TCD uses a lock per transport_trans_id (save_*_tcd_list_with_lock) to avoid duplicate rows (toll_a, toll_b).

use crate::models::ETDR::ETDR;
use crate::models::ETDR::{
    get_pending_tcd_bect, remove_pending_tcd_bect, save_bect_tcd_list_with_lock,
};
use crate::services::service::Service;
use crate::services::TransportTransactionStageService;

use super::build::build_transport_stage_from_etdr;
use super::merge::merge_etdr_into_transport_stage;

/// After BECT Stage is saved to DB: try to save pending TCD from KeyDB (if any) then remove the key.
async fn retry_pending_tcd_bect(transport_trans_id: i64) {
    let Some(list) = get_pending_tcd_bect(transport_trans_id).await else {
        return;
    };
    if list.is_empty() {
        return;
    }
    match save_bect_tcd_list_with_lock(transport_trans_id, &list).await {
        Ok(()) => {
            remove_pending_tcd_bect(transport_trans_id).await;
            tracing::info!(
                transport_trans_id,
                count = list.len(),
                "[Processor] BECT pending TCD saved to DB and key removed"
            );
        }
        Err(e) => {
            tracing::warn!(transport_trans_id, count = list.len(), error = %e, "[Processor] BECT pending TCD retry save failed, will retry next cycle");
        }
    }
}

/// Retry save DB cho một ETDR (BECT only).
/// Trả về true nếu save thành công, false nếu fail hoặc không đủ thông tin.
pub async fn retry_save_etdr_to_db(etdr: &ETDR) -> bool {
    let mut etdr = etdr.clone();
    etdr.etag_id = etdr.etag_id.trim().to_string();
    etdr.etag_number = etdr.etag_number.trim().to_string();

    if etdr.ticket_id == 0
        || etdr.station_id == 0
        || etdr.lane_id == 0
        || etdr.etag_number.is_empty()
    {
        tracing::warn!(ticket_id = etdr.ticket_id, station_id = etdr.station_id, lane_id = etdr.lane_id, etag = %etdr.etag_number, "[Processor] ETDR retry insufficient info");
        return false;
    }

    retry_save_bect(&etdr).await
}

async fn retry_save_bect(etdr: &ETDR) -> bool {
    let transport_service = TransportTransactionStageService::default();
    let ticket_id = etdr.ticket_id;

    // Single get_by_id: if record exists merge+update; otherwise continue to build+save.
    match transport_service.get_by_id(ticket_id).await {
        Ok(Some(existing_record)) => {
            let merged = merge_etdr_into_transport_stage(&existing_record, etdr);
            return match transport_service.update(ticket_id, &merged).await {
                Ok(true) => {
                    tracing::info!(transport_trans_id = ticket_id, etag = %etdr.etag_number, "[DB] ETDR retry TRANSPORT_TRANSACTION_STAGE merged and updated");
                    retry_pending_tcd_bect(ticket_id).await;
                    true
                }
                Ok(false) => {
                    tracing::info!(transport_trans_id = ticket_id, etag = %etdr.etag_number, "[DB] ETDR retry TRANSPORT_TRANSACTION_STAGE already in DB (update 0 rows), marking saved");
                    retry_pending_tcd_bect(ticket_id).await;
                    true
                }
                Err(e) => {
                    tracing::error!(transport_trans_id = ticket_id, etag = %etdr.etag_number, error = %e, "[DB] ETDR retry TRANSPORT_TRANSACTION_STAGE update failed");
                    false
                }
            };
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(transport_trans_id = ticket_id, ticket_id, etag = %etdr.etag_number, error = %e, "[DB] ETDR retry check TRANSPORT_TRANSACTION_STAGE exists failed, will try save");
        }
    }

    // Avoid double insert for same ticket_in_id: if record exists by ticket_in_id then merge+update instead of insert.
    if let Some(tid) = etdr.ticket_in_id {
        if tid != 0 {
            if let Ok(Some(existing_record)) = transport_service.get_by_ticket_in_id(tid).await {
                let merged = merge_etdr_into_transport_stage(&existing_record, etdr);
                return match transport_service
                    .update(existing_record.transport_trans_id, &merged)
                    .await
                {
                    Ok(true) => {
                        tracing::info!(
                            transport_trans_id = existing_record.transport_trans_id,
                            ticket_in_id = tid,
                            etag = %etdr.etag_number,
                            "[DB] ETDR retry TRANSPORT_TRANSACTION_STAGE merged into existing record by TICKET_IN_ID and updated"
                        );
                        retry_pending_tcd_bect(existing_record.transport_trans_id).await;
                        true
                    }
                    Ok(false) => {
                        tracing::info!(
                            transport_trans_id = existing_record.transport_trans_id,
                            ticket_in_id = tid,
                            etag = %etdr.etag_number,
                            "[DB] ETDR retry TRANSPORT_TRANSACTION_STAGE already in DB by TICKET_IN_ID (update 0 rows), marking saved"
                        );
                        retry_pending_tcd_bect(existing_record.transport_trans_id).await;
                        true
                    }
                    Err(e) => {
                        tracing::error!(
                            transport_trans_id = existing_record.transport_trans_id,
                            ticket_in_id = tid,
                            etag = %etdr.etag_number,
                            error = %e,
                            "[DB] ETDR retry TRANSPORT_TRANSACTION_STAGE update by TICKET_IN_ID failed"
                        );
                        false
                    }
                };
            }
        }
    }

    // Cross-node duplicate avoidance: no record in DB but etdr has ticket_in_id → skip insert (another node likely saved with different ticket_id).
    if etdr.ticket_in_id.is_some_and(|tid| tid != 0) {
        tracing::info!(
            ticket_id,
            ticket_in_id = ?etdr.ticket_in_id,
            etag = %etdr.etag_number,
            "[Processor] ETDR retry skip insert: no record in DB but ticket_in_id set (likely saved by another node)"
        );
        return true;
    }

    let transport_stage = build_transport_stage_from_etdr(etdr);

    match transport_service.save(&transport_stage).await {
        Ok(transport_trans_id) => {
            tracing::info!(transport_trans_id, ticket_id, etag = %etdr.etag_number, retry_count = etdr.db_save_retry_count, "[DB] ETDR retry TRANSPORT_TRANSACTION_STAGE saved");
            retry_pending_tcd_bect(transport_trans_id).await;
            true
        }
        Err(e) => {
            let error_msg = format!("{}", e);
            if error_msg.contains("duplicate")
                || error_msg.contains("unique")
                || error_msg.contains("already exists")
            {
                match transport_service.get_by_id(ticket_id).await {
                    Ok(Some(_)) => {
                        tracing::info!(transport_trans_id = ticket_id, ticket_id, etag = %etdr.etag_number, "[DB] ETDR retry TRANSPORT_TRANSACTION_STAGE already exists (duplicate), marking saved");
                        retry_pending_tcd_bect(ticket_id).await;
                        return true;
                    }
                    Ok(None) => {
                        if let Some(tid) = etdr.ticket_in_id {
                            if tid != 0 {
                                if let Ok(Some(existing)) =
                                    transport_service.get_by_ticket_in_id(tid).await
                                {
                                    tracing::info!(transport_trans_id = existing.transport_trans_id, ticket_in_id = tid, etag = %etdr.etag_number, "[DB] ETDR retry TRANSPORT_TRANSACTION_STAGE already exists by TICKET_IN_ID (duplicate), marking saved");
                                    retry_pending_tcd_bect(existing.transport_trans_id).await;
                                    return true;
                                }
                            }
                        }
                        tracing::warn!(ticket_id, etag = %etdr.etag_number, retry_count = etdr.db_save_retry_count, error = %e, "[DB] ETDR retry TRANSPORT_TRANSACTION_STAGE not found after duplicate");
                    }
                    Err(check_err) => {
                        tracing::error!(ticket_id, etag = %etdr.etag_number, retry_count = etdr.db_save_retry_count, error = %e, check_error = %check_err, "[DB] ETDR retry TRANSPORT_TRANSACTION_STAGE save and check failed");
                    }
                }
            } else {
                tracing::error!(ticket_id, etag = %etdr.etag_number, retry_count = etdr.db_save_retry_count, error = %e, "[DB] ETDR retry TRANSPORT_TRANSACTION_STAGE save failed");
            }
            false
        }
    }
}
