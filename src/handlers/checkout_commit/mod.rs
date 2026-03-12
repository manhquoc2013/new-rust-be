//! Handler CHECKOUT_COMMIT_BOO (3AZ): FE sends 188-byte request; backend returns CHECKOUT_COMMIT_BOO_RESP (3BZ, 96 bytes).
//! Per spec 2.3.1.7.19 / 2.3.1.7.20: exit-station Back-End requests card-issuer to commit checkout.

mod common;
mod handler;

use crate::constants::fe;
use crate::crypto::{create_encryptor_with_key, BlockEncryptMut, Pkcs7};
use crate::fe_protocol;
use crate::models::bect_messages::CHECKOUT_COMMIT_BOO_RESP;
use crate::models::TCOCmessages::FE_REQUEST;
use crate::utils::{timestamp_ms, wrap_encrypted_reply};
use std::error::Error;
use std::sync::Arc;

use crate::cache::config::cache_manager::CacheManager;
use crate::configs::pool_factory::OdbcConnectionManager;
use r2d2::Pool;

use self::common::{build_checkout_commit_boo_resp, parse_checkout_commit_boo};
use self::handler::process_checkout_commit_bect;

/// Handles CHECKOUT_COMMIT_BOO (3AZ): parse request, run BECT commit logic, build CHECKOUT_COMMIT_BOO_RESP (3BZ) and return encrypted.
/// Returns (response_bytes, status). Status 0 = success; non-zero = failure.
pub async fn handle_checkout_commit_boo(
    rq: FE_REQUEST,
    data: &[u8],
    conn_id: i32,
    encryption_key: &str,
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
) -> Result<(Vec<u8>, i32), Box<dyn Error>> {
    let req = parse_checkout_commit_boo(data)?;

    tracing::debug!(
        request_id = req.request_id,
        ticket_in_id = req.ticket_in_id,
        ticket_out_id = req.ticket_out_id,
        etag = %req.etag.trim(),
        "[BECT] CHECKOUT_COMMIT_BOO parsed"
    );

    let status = process_checkout_commit_bect(&req, conn_id, db_pool, cache).await?;

    let mut resp = CHECKOUT_COMMIT_BOO_RESP::default();
    resp.message_length = fe_protocol::response_checkout_commit_boo_resp_len();
    resp.command_id = fe::CHECKOUT_COMMIT_BOO_RESP;
    resp.version_id = req.version_id;
    resp.request_id = req.request_id;
    resp.session_id = req.session_id;
    resp.timestamp = timestamp_ms();
    resp.ticket_in_id = req.ticket_in_id;
    resp.hub_id = req.hub_id;
    resp.ticket_eTag_id = req.ticket_eTag_id;
    resp.ticket_out_id = req.ticket_out_id;
    resp.status = status;

    let buf = build_checkout_commit_boo_resp(&resp);
    let encryptor = create_encryptor_with_key(encryption_key);
    let encrypted = encryptor.encrypt_padded_vec_mut::<Pkcs7>(&buf);
    let reply = wrap_encrypted_reply(encrypted);

    tracing::info!(
        request_id = rq.request_id,
        ticket_in_id = req.ticket_in_id,
        ticket_out_id = req.ticket_out_id,
        etag = %req.etag.trim(),
        status = resp.status,
        "[BECT] CHECKOUT_COMMIT_BOO finished"
    );

    Ok((reply, resp.status))
}
