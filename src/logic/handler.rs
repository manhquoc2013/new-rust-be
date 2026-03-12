//! Task receives response from server (sau khi process_request xử lý) và forward reply to router để gửi tới đúng connection.
//! Response forwarding flow được giữ để mở rộng sau này (ví dụ: logging, metrics, transform response).

use crate::types::{IncomingMessage, ReplyToRoute};
use tokio::sync::mpsc::{Receiver, Sender};

/// Chạy vòng lặp nhận response (IncomingMessage từ server) và chuyển tiếp qua tx_logic_replies tới router.
pub async fn run_logic_handler(
    mut rx_client_requests: Receiver<IncomingMessage>,
    tx_logic_replies: Sender<ReplyToRoute>,
) {
    tracing::info!("[Processor] logic handler task started");

    #[allow(clippy::needless_range_loop)]
    while let Some(msg) = rx_client_requests.recv().await {
        let IncomingMessage {
            conn_id,
            command_id,
            request_id,
            data,
        } = msg;

        // TODO: Thêm business logic xử lý ở đây
        // Ví dụ: validate, transform, call external services, etc.

        let reply = ReplyToRoute {
            conn_id,
            request_id,
            data,
        };
        if let Err(e) = tx_logic_replies.send(reply).await {
            tracing::error!(conn_id, request_id, command_id, error = %e, "[Processor] cannot send reply to router");
            break;
        }
    }

    tracing::info!("[Processor] logic handler task ended");
}
