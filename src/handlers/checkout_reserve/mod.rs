//! Handler CHECKOUT_RESERVE_BOO (2AZ): FE sends variable-length request; backend returns CHECKOUT_RESERVE_BOO_RESP (2BZ, 100 bytes).
//! Per spec 2.3.1.7.17 / 2.3.1.7.18: exit-station Back-End requests card-issuer to reserve amount for checkout.

mod common;
mod handler;

use crate::constants::fe;
use crate::crypto::{create_encryptor_with_key, BlockEncryptMut, Pkcs7};
use crate::fe_protocol;
use crate::models::bect_messages::CHECKOUT_RESERVE_BOO_RESP;
use crate::models::TCOCmessages::FE_REQUEST;
use crate::utils::{timestamp_ms, wrap_encrypted_reply};
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

use crate::cache::config::cache_manager::CacheManager;
use crate::configs::pool_factory::OdbcConnectionManager;
use r2d2::Pool;

use self::common::{build_checkout_reserve_boo_resp, parse_checkout_reserve_boo};
use self::handler::process_checkout_reserve_bect;

/// Handles CHECKOUT_RESERVE_BOO (2AZ): parse request, run BECT reserve logic, build CHECKOUT_RESERVE_BOO_RESP (2BZ) and return encrypted.
/// Returns (response_bytes, status). Status 0 = success; non-zero = failure.
pub async fn handle_checkout_reserve_boo(
    rq: FE_REQUEST,
    data: &[u8],
    conn_id: i32,
    encryption_key: &str,
    db_pool: Arc<Pool<OdbcConnectionManager>>,
    cache: Arc<CacheManager>,
) -> Result<(Vec<u8>, i32), Box<dyn Error>> {
    let start = Instant::now();
    let req = parse_checkout_reserve_boo(data)?;

    tracing::debug!(
        request_id = req.request_id,
        ticket_in_id = req.ticket_in_id,
        ticket_out_id = req.ticket_out_id,
        etag = %req.etag.trim(),
        "[BECT] CHECKOUT_RESERVE_BOO parsed"
    );

    let (status, ticket_in_id, ticket_out_id) =
        process_checkout_reserve_bect(&req, conn_id, db_pool, cache).await?;

    let mut resp = CHECKOUT_RESERVE_BOO_RESP::default();
    resp.message_length = fe_protocol::response_checkout_reserve_boo_resp_len();
    resp.command_id = fe::CHECKOUT_RESERVE_BOO_RESP;
    resp.version_id = req.version_id;
    resp.request_id = req.request_id;
    resp.session_id = req.session_id;
    resp.timestamp = timestamp_ms();
    resp.process_time = start.elapsed().as_millis() as i32;
    resp.ticket_in_id = ticket_in_id;
    resp.hub_id = req.hub_id;
    resp.ticket_eTag_id = req.ticket_eTag_id;
    resp.ticket_out_id = ticket_out_id;
    resp.status = status;

    let buf = build_checkout_reserve_boo_resp(&resp);
    let encryptor = create_encryptor_with_key(encryption_key);
    let encrypted = encryptor.encrypt_padded_vec_mut::<Pkcs7>(&buf);
    let reply = wrap_encrypted_reply(encrypted);

    tracing::info!(
        request_id = rq.request_id,
        ticket_in_id = resp.ticket_in_id,
        ticket_out_id = resp.ticket_out_id,
        etag = %req.etag.trim(),
        status = resp.status,
        "[BECT] CHECKOUT_RESERVE_BOO finished"
    );

    Ok((reply, resp.status))
}
