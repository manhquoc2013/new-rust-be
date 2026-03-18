//! Shared types: ConnectionId, SessionId, ConnectionMap, SessionMap, message.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tokio::sync::mpsc::Sender;

/// Kiểu ID kết nối.
pub type ConnectionId = i32;

/// Kiểu ID phiên.
pub type SessionId = i64;

/// Map conn_id -> sender để gửi reply cho từng connection.
pub type ConnectionMap = Arc<Mutex<HashMap<ConnectionId, Sender<Vec<u8>>>>>;

/// Map conn_id -> encryption_key (để gửi TERMINATE_RESP on close/timeout).
pub type EncryptionKeyMap = Arc<Mutex<HashMap<ConnectionId, String>>>;

/// Thông tin theo dõi session (conn_id, thời điểm nhận cuối).
#[derive(Clone)]
pub struct SessionInfo {
    pub conn_id: ConnectionId,
    pub last_received_time: Instant,
}

/// Map session_id -> SessionInfo; RwLock cho đọc đồng thời.
pub type SessionMap = Arc<RwLock<HashMap<SessionId, SessionInfo>>>;

/// Các loại cập nhật session map.
#[derive(Debug, Clone)]
pub enum SessionUpdate {
    /// Thêm session mới.
    Insert {
        session_id: SessionId,
        conn_id: ConnectionId,
        last_received_time: Instant,
    },
    /// Cập nhật thời gian handshake cuối.
    UpdateHandshake {
        session_id: SessionId,
        last_received_time: Instant,
    },
    /// Xóa session.
    Remove { session_id: SessionId },
    /// Xóa mọi session theo conn_id.
    RemoveByConnId { conn_id: ConnectionId },
}

/// Channel gửi session updates đến task nền.
pub type SessionUpdateSender = tokio::sync::mpsc::UnboundedSender<SessionUpdate>;
#[allow(dead_code)]
pub type SessionUpdateReceiver = tokio::sync::mpsc::UnboundedReceiver<SessionUpdate>;

/// Message nhận được từ client (legacy; reader now sends RequestToProcess).
#[allow(dead_code)]
#[derive(Debug)]
pub struct IncomingMessage {
    pub conn_id: ConnectionId,
    pub command_id: i32,
    /// FE request_id from decrypted payload (8..16 or 8..12); 0 if not present.
    pub request_id: i64,
    pub data: Vec<u8>,
}

/// Reply to route to connection: conn_id + request_id (for async send and tracing).
#[derive(Debug, Clone)]
pub struct ReplyToRoute {
    pub conn_id: ConnectionId,
    pub request_id: i64,
    pub data: Vec<u8>,
}

/// Request to process: reader sends decrypted payload and context; processor pool runs process_request and sends ReplyToRoute.
#[derive(Debug)]
pub struct RequestToProcess {
    pub conn_id: ConnectionId,
    pub decrypted: Vec<u8>,
    pub encryption_key_str: String,
    pub toll_id: Option<String>,
    pub client_ip: Option<String>,
}
