//! Xử lý một kết nối TCP: handshake, đọc/ghi, route message tới logic.

use crate::cache::config::cache_manager::CacheManager;
use crate::configs::config::Config;
use crate::crypto::{create_decryptor_with_key, Aes128CbcDec, BlockDecryptMut, Pkcs7};
use crate::handlers::processor::process_request;
use crate::services::TcocConnectionServerService;
use crate::types::{
    ConnectionId, ConnectionMap, EncryptionKeyMap, IncomingMessage, SessionMap, SessionUpdateSender,
};
use bytes::BytesMut;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::Semaphore;

use super::client_ip::get_real_client_ip;
use super::terminate::{self as term};
use crate::constants::network::{self as net_consts};

/// Plain error frame sent before any encryption: 4 bytes length (LE) = 8, 4 bytes error_code (i32 LE).
/// Client can read and show error without decrypting. Call before closing socket when no encryption key is available.
pub async fn send_plain_error_then_close(socket: &mut TcpStream, error_code: i32) {
    let mut buf = [0u8; 8];
    buf[0..4].copy_from_slice(&8u32.to_le_bytes());
    buf[4..8].copy_from_slice(&error_code.to_le_bytes());
    let _ = socket.write_all(&buf).await;
    let _ = socket.flush().await;
}

/// Xử lý một kết nối TCP (handshake, đọc ghi, route tới logic).
/// Khi phát hiện client đóng kết nối (EOF, ConnectionReset, lỗi ghi), set `peer_disconnected = true`
/// để timeout handshake có thể bỏ qua gửi TERMINATE_RESP nếu kết nối đã không còn.
pub async fn handle_connection(
    conn_id: ConnectionId,
    mut socket: TcpStream,
    peer_addr: std::net::SocketAddr,
    tx_client_requests: Sender<IncomingMessage>,
    mut rx_socket_write: Receiver<Vec<u8>>,
    cache: Arc<CacheManager>,
    tx_session_updates: SessionUpdateSender,
    encryption_keys: EncryptionKeyMap,
    session_map: SessionMap,
    active_conns: ConnectionMap,
    peer_disconnected: Arc<AtomicBool>,
    reply_in_flight: Option<Arc<Semaphore>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client_ip = get_real_client_ip(&socket, peer_addr);
    tracing::debug!(conn_id, client_ip = %client_ip, "[Network] handling connection");

    socket = setup_keepalive(socket, conn_id, &active_conns).await?;

    let cfg = Config::get();
    let read_timeout = if cfg.socket_read_timeout_seconds > 0 {
        Some(Duration::from_secs(cfg.socket_read_timeout_seconds))
    } else {
        None
    };

    let tcoc_service = TcocConnectionServerService::new();
    let ip_str = client_ip.to_string();

    let mut buf = BytesMut::with_capacity(8192);

    // Ping/Pong trước khi check IP/encryption_key: đọc và trả Pong cho mọi bản tin Ping cho tới khi gặp dữ liệu khác.
    loop {
        match read_with_timeout(&mut socket, &mut buf, read_timeout, conn_id).await {
            Ok(0) => {
                peer_disconnected.store(true, Ordering::Relaxed);
                tracing::info!(conn_id, "[Network] client closed connection (EOF)");
                return Ok(());
            }
            Ok(_) => {
                if buf.len() >= 4 && is_plain_ping(&buf) {
                    let _ = buf.split_to(4);
                    let tx = {
                        let conns = active_conns.lock().unwrap();
                        conns.get(&conn_id).cloned()
                    };
                    if let Some(tx) = tx {
                        if tx.send(net_consts::PONG_RESPONSE.to_vec()).await.is_err() {
                            return Ok(());
                        }
                    }
                    tracing::debug!(conn_id, "[Network] plain Ping -> pong (pre-auth)");
                    continue;
                }
                break;
            }
            Err(e) => {
                match e.kind() {
                    std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset => {
                        peer_disconnected.store(true, Ordering::Relaxed);
                        tracing::info!(conn_id, error = %e, "[Network] client disconnected");
                        return Ok(());
                    }
                    _ => {
                        tracing::warn!(conn_id, error = %e, "[Network] pre-auth read error, proceeding to config check");
                    }
                }
                break;
            }
        }
    }

    let (toll_id, candidate_keys_vec) = load_server_config(conn_id, &tcoc_service, &ip_str).await;

    if candidate_keys_vec.is_empty() {
        return fail_no_encryption_key(conn_id, &ip_str, socket, active_conns).await;
    }

    let candidate_keys: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(candidate_keys_vec));
    let resolved_decryptor: Arc<Mutex<Option<(String, Aes128CbcDec)>>> = Arc::new(Mutex::new(None));

    let _key_guard = EncryptionKeyGuard {
        conn_id,
        encryption_keys: encryption_keys.clone(),
    };

    let has_empty_key = {
        let keys = candidate_keys.lock().unwrap();
        keys.iter().any(|k| k.trim().is_empty())
    };
    if has_empty_key {
        return fail_empty_key(conn_id, &ip_str, socket, active_conns).await;
    }
    let keys_count = {
        let keys = candidate_keys.lock().unwrap();
        let mut warn_len = false;
        for key in keys.iter() {
            if key.len() != 16 {
                warn_len = true;
                break;
            }
        }
        if warn_len {
            tracing::warn!(
                conn_id,
                expected = 16,
                "[Network] some encryption_key length not 16 (AES128), may cause decrypt issues"
            );
        }
        keys.len()
    };

    tracing::info!(
        conn_id,
        keys_count,
        ip = %ip_str,
        "[Network] candidate keys ready, decrypt on first encrypted message"
    );

    let pending_command: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));

    // Vòng chính: đọc (có thể append vào buf còn sót từ pre-auth), ghi reply, xử lý Ping/encrypted.
    loop {
        // Process any data already in buf so that first message need not be ping (pre-auth leaves non-ping data in buf).
        if buf.len() >= 4
            && !process_message_buffer(
                &mut buf,
                &candidate_keys,
                &resolved_decryptor,
                conn_id,
                &toll_id,
                &ip_str,
                &pending_command,
                &tx_client_requests,
                &tx_session_updates,
                &cache,
                &session_map,
                &encryption_keys,
                &active_conns,
                reply_in_flight.as_ref(),
            )
            .await
        {
            break;
        }

        tokio::select! {
            n = read_with_timeout(&mut socket, &mut buf, read_timeout, conn_id) => {
                match n {
                    Ok(0) => {
                        peer_disconnected.store(true, Ordering::Relaxed);
                        tracing::info!(conn_id, "[Network] client closed connection (EOF)");
                        break;
                    }
                    Ok(_) => {
                        if !process_message_buffer(
                            &mut buf,
                            &candidate_keys,
                            &resolved_decryptor,
                            conn_id,
                            &toll_id,
                            &ip_str,
                            &pending_command,
                            &tx_client_requests,
                            &tx_session_updates,
                            &cache,
                            &session_map,
                            &encryption_keys,
                            &active_conns,
                            reply_in_flight.as_ref(),
                        ).await {
                            break;
                        }
                    }
                    Err(e) => {
                        match e.kind() {
                            std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset => {
                                peer_disconnected.store(true, Ordering::Relaxed);
                                tracing::info!(conn_id, error = %e, "[Network] client disconnected");
                            }
                            std::io::ErrorKind::TimedOut => {
                                tracing::warn!(conn_id, error = %e, "[Network] read timeout");
                                term::send_terminate_resp_on_close(
                                    conn_id,
                                    session_map.clone(),
                                    encryption_keys.clone(),
                                    active_conns.clone(),
                                    term::status::ERROR_HANDSHAKE_TIMEOUT,
                                );
                                continue;
                            }
                            _ => {
                                tracing::error!(conn_id, error = %e, kind = ?e.kind(), "[Network] read error");
                            }
                        }
                        break;
                    }
                }
            }

            Some(data) = rx_socket_write.recv() => {
                if let Err(e) = socket.write_all(&data).await {
                    peer_disconnected.store(true, Ordering::Relaxed);
                    log_write_error(conn_id, "write", e);
                    break;
                }
                if let Err(e) = socket.flush().await {
                    peer_disconnected.store(true, Ordering::Relaxed);
                    log_write_error(conn_id, "flush", e);
                    break;
                }

                let should_close = {
                    let pending = pending_command.lock().unwrap();
                    *pending == Some(crate::constants::fe::TERMINATE)
                };

                if should_close {
                    tracing::info!(conn_id, "[Network] TERMINATE sent, closing connection");
                    break;
                }

                {
                    let mut pending = pending_command.lock().unwrap();
                    *pending = None;
                }
            }

            else => break,
        }
    }

    Ok(())
}

async fn setup_keepalive(
    socket: TcpStream,
    conn_id: ConnectionId,
    active_conns: &ConnectionMap,
) -> Result<TcpStream, Box<dyn Error>> {
    use socket2::SockRef;
    let cfg = Config::get();
    match socket.into_std() {
        Ok(std_stream) => {
            let sock_ref = SockRef::from(&std_stream);
            if let Err(e) = sock_ref.set_keepalive(true) {
                tracing::warn!(conn_id, error = %e, "[Network] failed to set SO_KEEPALIVE");
            } else if cfg.tcp_keepalive_idle_seconds > 0 {
                #[cfg(unix)]
                {
                    use socket2::TcpKeepalive;
                    let keepalive = TcpKeepalive::new()
                        .with_time(Duration::from_secs(cfg.tcp_keepalive_idle_seconds))
                        .with_interval(Duration::from_secs(cfg.tcp_keepalive_interval_seconds))
                        .with_retries(cfg.tcp_keepalive_retries);
                    if let Err(e) = sock_ref.set_tcp_keepalive(&keepalive) {
                        tracing::warn!(
                            conn_id,
                            error = %e,
                            "[Network] set_tcp_keepalive params failed (using OS default)"
                        );
                    } else {
                        tracing::debug!(
                            conn_id,
                            idle_secs = cfg.tcp_keepalive_idle_seconds,
                            interval_secs = cfg.tcp_keepalive_interval_seconds,
                            "[Network] TCP keepalive enabled (detect half-open across NAT/router)"
                        );
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = (
                        cfg.tcp_keepalive_interval_seconds,
                        cfg.tcp_keepalive_retries,
                    );
                    tracing::debug!(
                        conn_id,
                        "[Network] TCP keepalive enabled (platform default)"
                    );
                }
            } else {
                tracing::debug!(conn_id, "[Network] TCP keepalive enabled (OS default)");
            }
            match tokio::net::TcpStream::from_std(std_stream) {
                Ok(s) => Ok(s),
                Err(e) => {
                    tracing::error!(conn_id, error_code = term::status::ERROR_SOCKET_CONVERSION, error = %e, "[Network] failed to convert socket back to tokio, closing connection");
                    let mut conns = active_conns.lock().unwrap();
                    conns.remove(&conn_id);
                    Err(Box::new(e))
                }
            }
        }
        Err(_) => {
            tracing::warn!(
                conn_id,
                error_code = term::status::ERROR_SOCKET_CONVERSION,
                "[Network] failed to convert tokio socket to std for keepalive, closing connection"
            );
            let mut conns = active_conns.lock().unwrap();
            conns.remove(&conn_id);
            Err(Box::new(std::io::Error::other(format!(
                "Error {}: Failed to setup keepalive",
                term::status::ERROR_SOCKET_CONVERSION
            ))))
        }
    }
}

/// Ghép danh sách key từ chuỗi (hỗ trợ "key_1,key_2,key_3") vào `out`.
fn collect_encryption_keys(s: &str, out: &mut Vec<String>) {
    for part in s.split(',') {
        let t = part.trim();
        if !t.is_empty() {
            out.push(t.to_string());
        }
    }
}

async fn load_server_config(
    conn_id: ConnectionId,
    tcoc_service: &TcocConnectionServerService,
    ip_str: &str,
) -> (Option<String>, Vec<String>) {
    match tcoc_service.get_all_by_ip(ip_str).await {
        Ok(list) if !list.is_empty() => {
            let toll_id = list.first().and_then(|c| c.toll_id.clone());
            let mut candidate_keys = Vec::new();
            for record in &list {
                if let Some(ref s) = record.encryption_key {
                    collect_encryption_keys(s, &mut candidate_keys);
                }
            }
            if candidate_keys.is_empty() {
                tracing::warn!(
                    conn_id,
                    ip = %ip_str,
                    records = list.len(),
                    "[Network] server config has no valid encryption_key (null or empty)"
                );
            }
            tracing::debug!(
                conn_id,
                toll_id = ?toll_id,
                keys_count = candidate_keys.len(),
                "[Network] found server config"
            );
            (toll_id, candidate_keys)
        }
        Ok(_) => {
            tracing::warn!(conn_id, ip = %ip_str, "[Network] no server config for IP");
            (None, Vec::new())
        }
        Err(e) => {
            tracing::error!(conn_id, ip = %ip_str, error = ?e, "[Network] server config query failed");
            (None, Vec::new())
        }
    }
}

async fn fail_no_encryption_key(
    conn_id: ConnectionId,
    ip_str: &str,
    mut socket: TcpStream,
    active_conns: ConnectionMap,
) -> Result<(), Box<dyn Error>> {
    tracing::error!(conn_id, error_code = term::status::ERROR_NO_ENCRYPTION_KEY, ip = %ip_str, "[Network] no encryption_key for IP, closing connection");
    {
        let mut conns = active_conns.lock().unwrap();
        conns.remove(&conn_id);
    }
    send_plain_error_then_close(&mut socket, term::status::ERROR_NO_ENCRYPTION_KEY).await;
    drop(socket);
    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        format!(
            "Error {}: encryption_key not found for IP {}",
            term::status::ERROR_NO_ENCRYPTION_KEY,
            ip_str
        ),
    )))
}

async fn fail_empty_key(
    conn_id: ConnectionId,
    ip_str: &str,
    mut socket: TcpStream,
    active_conns: ConnectionMap,
) -> Result<(), Box<dyn Error>> {
    tracing::error!(
        conn_id,
        error_code = term::status::ERROR_EMPTY_KEY,
        "[Network] encryption_key empty after trim, closing connection"
    );
    {
        let mut conns = active_conns.lock().unwrap();
        conns.remove(&conn_id);
    }
    send_plain_error_then_close(&mut socket, term::status::ERROR_EMPTY_KEY).await;
    drop(socket);
    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!(
            "Error {}: encryption_key is empty for IP {} (reason=empty_key)",
            term::status::ERROR_EMPTY_KEY,
            ip_str
        ),
    )))
}

struct EncryptionKeyGuard {
    conn_id: ConnectionId,
    encryption_keys: EncryptionKeyMap,
}

impl Drop for EncryptionKeyGuard {
    fn drop(&mut self) {
        let mut keys = self.encryption_keys.lock().unwrap();
        keys.remove(&self.conn_id);
        tracing::debug!(
            conn_id = self.conn_id,
            "[Network] cleared encryption_key from map"
        );
    }
}

/// Đọc từ socket với timeout (nếu cấu hình). Trả về Err(TimedOut) khi timeout; caller gửi TERMINATE_RESP khi cần.
async fn read_with_timeout(
    socket: &mut TcpStream,
    buf: &mut BytesMut,
    read_timeout: Option<Duration>,
    conn_id: ConnectionId,
) -> Result<usize, std::io::Error> {
    if let Some(timeout) = read_timeout {
        match tokio::time::timeout(timeout, socket.read_buf(buf)).await {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(
                    conn_id,
                    timeout_secs = timeout.as_secs(),
                    "[Network] socket read timeout"
                );
                Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("Read timeout after {}s", timeout.as_secs()),
                ))
            }
        }
    } else {
        socket.read_buf(buf).await
    }
}

/// Trả về true nếu 4 byte đầu là "ping" không phân biệt hoa/thường (ASCII); không kiểm tra UTF-8.
#[inline(always)]
fn is_plain_ping(buf: &[u8]) -> bool {
    buf.len() >= 4
        && (buf[0] == b'p' || buf[0] == b'P')
        && (buf[1] == b'i' || buf[1] == b'I')
        && (buf[2] == b'n' || buf[2] == b'N')
        && (buf[3] == b'g' || buf[3] == b'G')
}

#[allow(clippy::too_many_arguments)]
async fn process_message_buffer(
    buf: &mut BytesMut,
    candidate_keys: &Arc<Mutex<Vec<String>>>,
    resolved_decryptor: &Arc<Mutex<Option<(String, Aes128CbcDec)>>>,
    conn_id: ConnectionId,
    toll_id: &Option<String>,
    ip_str: &str,
    pending_command: &Arc<Mutex<Option<i32>>>,
    tx_client_requests: &Sender<IncomingMessage>,
    tx_session_updates: &SessionUpdateSender,
    cache: &Arc<CacheManager>,
    session_map: &SessionMap,
    encryption_keys: &EncryptionKeyMap,
    active_conns: &ConnectionMap,
    reply_in_flight: Option<&Arc<Semaphore>>,
) -> bool {
    let mut consecutive_errors = 0;

    loop {
        if buf.len() < 4 {
            break;
        }

        // Ping plain-text: 4 byte đầu là "ping" (không phân biệt hoa/thường); trả về "pong".
        // Mỗi vòng chỉ xử lý một frame (consume 4 byte), phần còn lại giữ nguyên. Đang nhận dở frame
        // (buf.len() < total_message_length) thì break không consume, lần read sau append; 4 byte đầu vẫn là length (luồng encrypt).
        if is_plain_ping(buf) {
            let _ = buf.split_to(4);
            let tx = {
                let conns = active_conns.lock().unwrap();
                conns.get(&conn_id).cloned()
            };
            if let Some(tx) = tx {
                if tx.send(net_consts::PONG_RESPONSE.to_vec()).await.is_err() {
                    return false;
                }
            }
            tracing::debug!(conn_id, "[Network] plain Ping -> pong");
            continue;
        }

        let total_message_length = match buf[0..4].try_into() {
            Ok(bytes) => u32::from_le_bytes(bytes) as usize,
            Err(_) => {
                tracing::warn!(
                    conn_id,
                    buf_len = buf.len(),
                    "[Network] failed to parse length header"
                );
                term::send_terminate_resp_on_close(
                    conn_id,
                    session_map.clone(),
                    encryption_keys.clone(),
                    active_conns.clone(),
                    term::status::ERROR_INVALID_MESSAGE,
                );
                buf.clear();
                return false;
            }
        };

        if !(4..=net_consts::MAX_MESSAGE_SIZE).contains(&total_message_length) {
            tracing::warn!(
                conn_id,
                message_length = total_message_length,
                buf_len = buf.len(),
                "[Network] invalid message length"
            );
            let _ = buf.split_to(1);
            consecutive_errors += 1;
            if consecutive_errors >= net_consts::MAX_CONSECUTIVE_ERRORS {
                tracing::warn!(
                    conn_id,
                    "[Network] too many consecutive errors, clearing buffer"
                );
                term::send_terminate_resp_on_close(
                    conn_id,
                    session_map.clone(),
                    encryption_keys.clone(),
                    active_conns.clone(),
                    term::status::ERROR_INVALID_MESSAGE,
                );
                buf.clear();
                return false;
            }
            continue;
        }

        if buf.len() < total_message_length {
            break;
        }

        let message_data = buf.split_to(total_message_length).freeze().to_vec();
        if message_data.len() != total_message_length {
            tracing::warn!(
                conn_id,
                expected = total_message_length,
                got = message_data.len(),
                "[Network] message extraction size mismatch"
            );
            consecutive_errors += 1;
            if consecutive_errors >= net_consts::MAX_CONSECUTIVE_ERRORS {
                tracing::warn!(
                    conn_id,
                    "[Network] too many consecutive errors, clearing buffer"
                );
                term::send_terminate_resp_on_close(
                    conn_id,
                    session_map.clone(),
                    encryption_keys.clone(),
                    active_conns.clone(),
                    term::status::ERROR_INVALID_MESSAGE,
                );
                buf.clear();
                return false;
            }
            continue;
        }

        consecutive_errors = 0;
        let payload = &message_data[4..];

        let (decrypted, encryption_key_str) = {
            let resolved_guard = resolved_decryptor.lock().unwrap();
            if let Some((ref key, ref decryptor)) = *resolved_guard {
                match decryptor.clone().decrypt_padded_vec_mut::<Pkcs7>(payload) {
                    Ok(d) => (d, key.clone()),
                    Err(e) => {
                        drop(resolved_guard);
                        tracing::error!(conn_id, error_code = term::status::ERROR_DECRYPT_FAILED, error = ?e, "[Network] AES decrypt failed");
                        term::send_terminate_resp_on_close(
                            conn_id,
                            session_map.clone(),
                            encryption_keys.clone(),
                            active_conns.clone(),
                            term::status::ERROR_DECRYPT_FAILED,
                        );
                        continue;
                    }
                }
            } else {
                let keys_to_try: Vec<String> = candidate_keys.lock().unwrap().clone();
                drop(resolved_guard);
                let mut decrypted_opt = None;
                let mut resolved_key = None;
                for key in keys_to_try.iter() {
                    let decryptor = create_decryptor_with_key(key);
                    if let Ok(decrypted_vec) = decryptor.decrypt_padded_vec_mut::<Pkcs7>(payload) {
                        {
                            let mut guard = resolved_decryptor.lock().unwrap();
                            *guard = Some((key.clone(), create_decryptor_with_key(key)));
                        }
                        {
                            let mut keys = encryption_keys.lock().unwrap();
                            keys.insert(conn_id, key.clone());
                        }
                        candidate_keys.lock().unwrap().clear();
                        tracing::info!(
                            conn_id,
                            keys_tried = keys_to_try.len(),
                            "[Network] decryption key resolved for connection"
                        );
                        decrypted_opt = Some(decrypted_vec);
                        resolved_key = Some(key.clone());
                        break;
                    }
                }
                match (decrypted_opt, resolved_key) {
                    (Some(d), Some(k)) => (d, k),
                    _ => {
                        tracing::error!(
                            conn_id,
                            error_code = term::status::ERROR_DECRYPT_FAILED,
                            keys_tried = keys_to_try.len(),
                            "[Network] AES decrypt failed with all candidate keys"
                        );
                        term::send_terminate_resp_on_close(
                            conn_id,
                            session_map.clone(),
                            encryption_keys.clone(),
                            active_conns.clone(),
                            term::status::ERROR_DECRYPT_FAILED,
                        );
                        continue;
                    }
                }
            }
        };

        if decrypted.len() < 8 {
            tracing::warn!(
                conn_id,
                len = decrypted.len(),
                min = 8,
                "[Network] decrypted data too short"
            );
            continue;
        }

        let (inner_message_length, command_id) =
            match (decrypted[0..4].try_into(), decrypted[4..8].try_into()) {
                (Ok(len_bytes), Ok(cmd_bytes)) => {
                    (i32::from_le_bytes(len_bytes), i32::from_le_bytes(cmd_bytes))
                }
                _ => {
                    tracing::warn!(
                        conn_id,
                        decrypted_len = decrypted.len(),
                        "[Network] failed to parse message header"
                    );
                    continue;
                }
            };

        let request_id = if decrypted.len() >= 16 {
            i64::from_le_bytes(decrypted[8..16].try_into().unwrap())
        } else {
            0
        };

        tracing::debug!(
            conn_id,
            command_id,
            request_id,
            len = inner_message_length,
            "[Network] request"
        );

        {
            let mut pending = pending_command.lock().unwrap();
            *pending = Some(command_id);
        }

        // Back-pressure: acquire permit before process_request so we never overfill the reply channel.
        let _permit = if let Some(sem) = reply_in_flight {
            Some(
                sem.clone()
                    .acquire_owned()
                    .await
                    .expect("reply_in_flight semaphore closed"),
            )
        } else {
            None
        };

        let reply_bytes = match process_request(
            decrypted,
            conn_id,
            command_id,
            cache.clone(),
            toll_id.clone(),
            tx_session_updates.clone(),
            &encryption_key_str,
            Some(ip_str.to_string()),
        )
        .await
        {
            Ok(rb) => rb,
            Err(e) => {
                tracing::error!(conn_id, error = %e, "[Network] process_request failed");
                let mut pending = pending_command.lock().unwrap();
                *pending = None;
                // Client receives no reply for this request (permit is released on continue).
                continue;
            }
        };

        if tx_client_requests
            .send(IncomingMessage {
                conn_id,
                command_id,
                request_id,
                data: reply_bytes,
            })
            .await
            .is_err()
        {
            return false;
        }
    }

    true
}

fn log_write_error(conn_id: ConnectionId, op: &str, e: std::io::Error) {
    match e.kind() {
        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset => {
            tracing::info!(conn_id, error = %e, "[Network] client disconnected during {}", op);
        }
        _ => {
            tracing::error!(conn_id, error = %e, kind = ?e.kind(), "[Network] {} error", op);
        }
    }
}
