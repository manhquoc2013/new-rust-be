//! CONNECT handler: FE gửi CONNECT (req), backend trả CONNECT_RESP (resp) per spec 2.3.1.7.1 / 2.3.1.7.2.
//! Flow: decrypt → validate length → auth (or bypass) → allocate session_id → save TCOC_SESSIONS → send CONNECT_RESP (32 bytes).

use crate::configs::mediation_db::MEDIATION_DB;
use crate::constants::fe;
use crate::crypto::{create_encryptor_with_key, Aes128CbcEnc, BlockEncryptMut, Pkcs7};
use crate::db::repositories::TcocSession;
use crate::db::sequence::get_next_sequence_value_with_schema;
use crate::fe_protocol;
use crate::models::TCOCmessages::{FE_CONNECT, FE_CONNECT_RESP, FE_REQUEST};
use crate::services::{service::Service, TcocSessionService, TcocUserService};
use crate::types::{SessionUpdate, SessionUpdateSender};
use crate::utils::{normalize_etag, wrap_encrypted_reply};
use base64::Engine;
use sha1::Digest;
use std::error::Error;
use std::time::Instant;
use tokio::io::AsyncWriteExt;

/// Sends FE_CONNECT_RESP with error status (e.g. 301, 305, 306). Message length 32 bytes (2.3.1.7.2).
async fn send_connect_error(
    encryptor: &Aes128CbcEnc,
    version_id: i32,
    request_id: i64,
    status: i32,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut fe_connect_resp: FE_CONNECT_RESP = FE_CONNECT_RESP::default();
    fe_connect_resp.message_length = fe_protocol::CONNECT_RESP_LEN;
    fe_connect_resp.command_id = fe::CONNECT_RESP;
    fe_connect_resp.version_id = version_id;
    fe_connect_resp.request_id = request_id;
    fe_connect_resp.session_id = 0;
    fe_connect_resp.status = status;
    if tracing::enabled!(tracing::Level::DEBUG) {
        tracing::debug!(
            request_id = fe_connect_resp.request_id,
            session_id = fe_connect_resp.session_id,
            command_id = fe_connect_resp.command_id,
            status = fe_connect_resp.status,
            "[FE] CONNECT_RESP returning to client (error)"
        );
    }
    let cap = fe_connect_resp.message_length as usize;
    let mut buffer_write = Vec::with_capacity(cap);
    buffer_write
        .write_i32_le(fe_connect_resp.message_length)
        .await?;
    buffer_write
        .write_i32_le(fe_connect_resp.command_id)
        .await?;
    fe_protocol::write_connect_resp_body(
        &mut buffer_write,
        fe_connect_resp.version_id,
        request_id,
        0,
        fe_connect_resp.status,
    )
    .await?;
    let encrypted_reply = encryptor
        .clone()
        .encrypt_padded_vec_mut::<Pkcs7>(&buffer_write);
    Ok(wrap_encrypted_reply(encrypted_reply))
}

/// Handles CONNECT: parse FE_CONNECT (44 bytes), authenticate, create session, return FE_CONNECT_RESP (32 bytes).
/// `client_ip`: client (FE/gate) IP stored in TCOC_SESSIONS.IP_ADDRESS.
pub async fn handle_connect(
    rq: FE_REQUEST,
    data: Vec<u8>,
    conn_id: i32,
    tx_session_updates: SessionUpdateSender,
    encryption_key: &str,
    client_ip: Option<&str>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let encryptor = create_encryptor_with_key(encryption_key);
    let decrypted = data.clone();

    if decrypted.len() < fe_protocol::len::CONNECT {
        return Err(format!(
            "CONNECT message too short: {} bytes (minimum {})",
            decrypted.len(),
            fe_protocol::len::CONNECT
        )
        .into());
    }

    let mut fe_connect: FE_CONNECT = FE_CONNECT::default();
    fe_connect.message_length = rq.message_length;
    fe_connect.command_id = rq.command_id;
    fe_connect.version_id = i32::from_le_bytes(decrypted[8..12].try_into().unwrap());
    fe_connect.request_id = i64::from_le_bytes(decrypted[12..20].try_into().unwrap());
    fe_connect.username = String::from_utf8_lossy(&decrypted[20..30]).to_string();
    fe_connect.password = String::from_utf8_lossy(&decrypted[30..40]).to_string();
    fe_connect.timeout = i32::from_le_bytes(decrypted[40..44].try_into().unwrap());

    tracing::debug!(
        conn_id,
        request_id = fe_connect.request_id,
        "[Network] FE_CONNECT decrypted"
    );

    // Allocate SESSION_ID early so TCOC_SESSIONS can be written on both success and failure. spawn_blocking avoids blocking async runtime (DB I/O).
    let session_id_from_seq = match tokio::task::spawn_blocking(|| {
        get_next_sequence_value_with_schema(&MEDIATION_DB, "MEDIATION_OWNER", "TCOC_SESSIONS_SEQ")
    })
    .await
    {
        Ok(Ok(session_id)) => {
            tracing::info!(
                conn_id,
                request_id = fe_connect.request_id,
                session_id,
                "[Network] SESSION_ID from sequence"
            );
            session_id
        }
        Ok(Err(e)) => {
            tracing::error!(conn_id, request_id = fe_connect.request_id, error = %e, "[Network] sequence error, using conn_id");
            conn_id as i64
        }
        Err(join_e) => {
            tracing::error!(conn_id, request_id = fe_connect.request_id, error = %join_e, "[Network] sequence spawn_blocking failed, using conn_id");
            conn_id as i64
        }
    };
    let datetime_str = crate::utils::now_utc_db_string();

    let bypass_auth =
        crate::utils::parse_env_bool_loose(std::env::var("CONNECT_BYPASS_AUTH").ok().as_deref());

    let username = normalize_etag(&fe_connect.username);
    let toll_id = 0_i64;

    let mut authenticated_user_id: Option<i64> = None;
    if !bypass_auth {
        let password = normalize_etag(&fe_connect.password);

        let user_service = TcocUserService::new();
        match user_service
            .get_by_username_and_toll_id(&username, toll_id)
            .await
        {
            Ok(Some(user)) => {
                if user.status.as_deref() != Some("1") {
                    tracing::error!(conn_id, request_id = fe_connect.request_id, username = %username, toll_id, user_status = ?user.status, "[Network] CONNECT account not activated");
                    let failure_session = TcocSession {
                        session_id: session_id_from_seq,
                        user_id: Some(user.user_id),
                        init_datetime: Some(datetime_str.clone()),
                        login_datetime: Some(datetime_str.clone()),
                        logout_datetime: Some(datetime_str.clone()),
                        valid_code: Some("INVALID".to_string()),
                        ip_address: client_ip.map(String::from),
                        user_name: Some(normalize_etag(&fe_connect.username)),
                        toll_id: Some(0),
                        status: Some("FAILED".to_string()),
                        description: Some("CONNECT account not activated".to_string()),
                        server_id: Some(1),
                    };
                    if let Err(e) = TcocSessionService::new().save(&failure_session).await {
                        tracing::error!(session_id = session_id_from_seq, error = %e, "[Network] TCOC_SESSIONS save failure session failed");
                    }
                    return send_connect_error(
                        &encryptor,
                        fe_connect.version_id,
                        fe_connect.request_id,
                        fe::ACCOUNT_NOT_ACTIVATED,
                    )
                    .await;
                }
                let mut hasher = sha1::Sha1::new();
                hasher.update(password.as_bytes());
                let hash_bytes = hasher.finalize();
                let hash_password = base64::engine::general_purpose::STANDARD.encode(hash_bytes);
                if user.password != hash_password {
                    tracing::error!(conn_id, request_id = fe_connect.request_id, username = %username, toll_id, "[Network] CONNECT invalid password");
                    let failure_session = TcocSession {
                        session_id: session_id_from_seq,
                        user_id: Some(user.user_id),
                        init_datetime: Some(datetime_str.clone()),
                        login_datetime: Some(datetime_str.clone()),
                        logout_datetime: Some(datetime_str.clone()),
                        valid_code: Some("INVALID".to_string()),
                        ip_address: client_ip.map(String::from),
                        user_name: Some(normalize_etag(&fe_connect.username)),
                        toll_id: Some(0),
                        status: Some("FAILED".to_string()),
                        description: Some("CONNECT invalid password".to_string()),
                        server_id: Some(1),
                    };
                    if let Err(e) = TcocSessionService::new().save(&failure_session).await {
                        tracing::error!(session_id = session_id_from_seq, error = %e, "[Network] TCOC_SESSIONS save failure session failed");
                    }
                    return send_connect_error(
                        &encryptor,
                        fe_connect.version_id,
                        fe_connect.request_id,
                        fe::NOT_FOUND_STATION_LANE,
                    )
                    .await;
                }
                authenticated_user_id = Some(user.user_id);
                tracing::info!(conn_id, request_id = fe_connect.request_id, username = %username, toll_id, "[Network] CONNECT user authenticated");
            }
            Ok(None) => {
                tracing::error!(conn_id, request_id = fe_connect.request_id, username = %username, toll_id, "[Network] CONNECT user not found");
                let failure_session = TcocSession {
                    session_id: session_id_from_seq,
                    user_id: None,
                    init_datetime: Some(datetime_str.clone()),
                    login_datetime: Some(datetime_str.clone()),
                    logout_datetime: Some(datetime_str.clone()),
                    valid_code: Some("INVALID".to_string()),
                    ip_address: client_ip.map(String::from),
                    user_name: Some(normalize_etag(&fe_connect.username)),
                    toll_id: Some(0),
                    status: Some("FAILED".to_string()),
                    description: Some("CONNECT user not found".to_string()),
                    server_id: Some(1),
                };
                if let Err(e) = TcocSessionService::new().save(&failure_session).await {
                    tracing::error!(session_id = session_id_from_seq, error = %e, "[Network] TCOC_SESSIONS save failure session failed");
                }
                return send_connect_error(
                    &encryptor,
                    fe_connect.version_id,
                    fe_connect.request_id,
                    fe::USER_NOT_FOUND,
                )
                .await;
            }
            Err(e) => {
                tracing::error!(conn_id, request_id = fe_connect.request_id, username = %username, toll_id, error = ?e, "[Network] CONNECT query user failed");
                let failure_session = TcocSession {
                    session_id: session_id_from_seq,
                    user_id: None,
                    init_datetime: Some(datetime_str.clone()),
                    login_datetime: Some(datetime_str.clone()),
                    logout_datetime: Some(datetime_str.clone()),
                    valid_code: Some("INVALID".to_string()),
                    ip_address: client_ip.map(String::from),
                    user_name: Some(normalize_etag(&fe_connect.username)),
                    toll_id: Some(0),
                    status: Some("FAILED".to_string()),
                    description: Some("CONNECT query user failed".to_string()),
                    server_id: Some(1),
                };
                if let Err(e) = TcocSessionService::new().save(&failure_session).await {
                    tracing::error!(session_id = session_id_from_seq, error = %e, "[Network] TCOC_SESSIONS save failure session failed");
                }
                return send_connect_error(
                    &encryptor,
                    fe_connect.version_id,
                    fe_connect.request_id,
                    fe::NOT_FOUND_STATION_LANE,
                )
                .await;
            }
        }
    } else {
        tracing::info!(conn_id, request_id = fe_connect.request_id, username = %username, toll_id, "[Network] CONNECT_BYPASS_AUTH enabled");
    }

    // Save TCOC_SESSIONS on successful connect; logout_datetime updated when connection closes.
    let new_session = TcocSession {
        session_id: session_id_from_seq,
        user_id: authenticated_user_id,
        init_datetime: Some(datetime_str.clone()),
        login_datetime: Some(datetime_str.clone()),
        logout_datetime: None,
        valid_code: Some("VALID".to_string()),
        ip_address: client_ip.map(String::from),
        user_name: Some(normalize_etag(&fe_connect.username)),
        toll_id: Some(0),
        status: Some("ACTIVE".to_string()),
        description: Some("FE_CONNECT".to_string()),
        server_id: Some(1),
    };

    let service = TcocSessionService::new();
    match service.save(&new_session).await {
        Ok(inserted_session_id) => {
            tracing::info!(
                conn_id,
                request_id = fe_connect.request_id,
                session_id = inserted_session_id,
                "[Network] session saved"
            );
        }
        Err(e) => {
            tracing::error!(conn_id, request_id = fe_connect.request_id, error = %e, "[Network] save session failed");
        }
    }

    let mut fe_connect_resp: FE_CONNECT_RESP = FE_CONNECT_RESP::default();
    fe_connect_resp.message_length = fe_protocol::CONNECT_RESP_LEN;
    fe_connect_resp.command_id = fe::CONNECT_RESP;
    fe_connect_resp.version_id = fe_connect.version_id;
    fe_connect_resp.request_id = fe_connect.request_id;
    fe_connect_resp.session_id = session_id_from_seq;
    fe_connect_resp.status = 0;

    if tracing::enabled!(tracing::Level::DEBUG) {
        tracing::debug!(
            request_id = fe_connect_resp.request_id,
            session_id = fe_connect_resp.session_id,
            command_id = fe_connect_resp.command_id,
            status = fe_connect_resp.status,
            "[FE] CONNECT_RESP returning to client"
        );
    }

    {
        let session_id = fe_connect_resp.session_id;
        let last_received_time = Instant::now();
        let _ = tx_session_updates.send(SessionUpdate::Insert {
            session_id,
            conn_id,
            last_received_time,
        });
        tracing::info!(
            request_id = fe_connect.request_id,
            session_id = fe_connect_resp.session_id,
            conn_id,
            "[Network] session registered"
        );
    }

    tracing::debug!(
        conn_id,
        request_id = fe_connect.request_id,
        "[Network] sending FE_CONNECT_RESP"
    );

    let cap = fe_connect_resp.message_length as usize;
    let mut buffer_write = Vec::with_capacity(cap);
    buffer_write
        .write_i32_le(fe_connect_resp.message_length)
        .await?;
    buffer_write
        .write_i32_le(fe_connect_resp.command_id)
        .await?;
    fe_protocol::write_connect_resp_body(
        &mut buffer_write,
        fe_connect_resp.version_id,
        fe_connect_resp.request_id,
        fe_connect_resp.session_id,
        fe_connect_resp.status,
    )
    .await?;

    let encrypted_reply = encryptor
        .clone()
        .encrypt_padded_vec_mut::<Pkcs7>(&buffer_write);
    let reply_bytes = wrap_encrypted_reply(encrypted_reply);

    tracing::debug!(
        conn_id,
        request_id = fe_connect.request_id,
        len = reply_bytes.len(),
        "[Network] FE_CONNECT_RESP sent"
    );

    Ok(reply_bytes)
}
