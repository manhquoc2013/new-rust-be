//! Handler CHECKOUT_ROLLBACK_BOO (3AZ): FE sends 178-byte request; backend returns CHECKOUT_ROLLBACK_BOO_RESP (3BZ, 96 bytes).
//! Per spec 2.3.1.7.21 / 2.3.1.7.22: exit-station Back-End requests card-issuer to rollback checkout.

mod common;

use crate::constants::fe;
use crate::crypto::{create_encryptor_with_key, BlockEncryptMut, Pkcs7};
use crate::fe_protocol;
use crate::models::bect_messages::CHECKOUT_ROLLBACK_BOO_RESP;
use crate::models::TCOCmessages::FE_REQUEST;
use crate::utils::{timestamp_ms, wrap_encrypted_reply};
use std::error::Error;

use self::common::{build_checkout_rollback_boo_resp, parse_checkout_rollback_boo};

/// Handles CHECKOUT_ROLLBACK_BOO (3AZ): parse request, normalize content, build CHECKOUT_ROLLBACK_BOO_RESP (3BZ) and return encrypted.
/// Returns (response_bytes, status). Status 0 = success; non-zero = failure.
pub async fn handle_checkout_rollback_boo(
    rq: FE_REQUEST,
    data: &[u8],
    _conn_id: i32,
    encryption_key: &str,
) -> Result<(Vec<u8>, i32), Box<dyn Error>> {
    let req = parse_checkout_rollback_boo(data)?;

    tracing::debug!(
        request_id = req.request_id,
        ticket_in_id = req.ticket_in_id,
        ticket_out_id = req.ticket_out_id,
        etag = %req.etag.trim(),
        "[BECT] CHECKOUT_ROLLBACK_BOO parsed"
    );

    let mut resp = CHECKOUT_ROLLBACK_BOO_RESP::default();
    resp.message_length = fe_protocol::response_checkout_rollback_boo_resp_len();
    resp.command_id = fe::CHECKOUT_ROLLBACK_BOO_RESP;
    resp.version_id = req.version_id;
    resp.request_id = req.request_id;
    resp.session_id = req.session_id;
    resp.timestamp = timestamp_ms();
    resp.ticket_in_id = req.ticket_in_id;
    resp.hub_id = req.hub_id;
    resp.ticket_eTag_id = req.ticket_eTag_id;
    resp.ticket_out_id = req.ticket_out_id;
    resp.status = 0; // success; spec: 0 = success

    let buf = build_checkout_rollback_boo_resp(&resp);
    let encryptor = create_encryptor_with_key(encryption_key);
    let encrypted = encryptor.encrypt_padded_vec_mut::<Pkcs7>(&buf);
    let reply = wrap_encrypted_reply(encrypted);

    tracing::info!(
        request_id = rq.request_id,
        ticket_in_id = req.ticket_in_id,
        ticket_out_id = req.ticket_out_id,
        etag = %req.etag.trim(),
        "[BECT] CHECKOUT_ROLLBACK_BOO finished"
    );

    Ok((reply, resp.status))
}
