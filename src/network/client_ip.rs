//! Lấy địa chỉ IP client từ kết nối TCP.

use std::net::IpAddr;
use tokio::net::TcpStream;

/// Get client IP from socket (source address of TCP connection).
///
/// Luôn dùng `peer_addr.ip()`: đây là IP của client (FE/gate). Giá trị này dùng để lookup
/// TCOC_CONNECTION_SERVER (toll_id, encryption_key) — bảng lưu theo IP của FE kết nối tới server.
///
/// Lưu ý: SO_ORIGINAL_DST (Linux) trả về *original destination* (đích trước REDIRECT/DNAT),
/// không phải client IP; dùng nó cho lookup config sẽ sai (vd. trong K8s có thể ra Service IP).
pub fn get_real_client_ip(_socket: &TcpStream, peer_addr: std::net::SocketAddr) -> IpAddr {
    peer_addr.ip()
}
