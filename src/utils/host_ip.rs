//! Get worker IP (pod) cho ứng dụng chạy trong K8s (auto-detect via env).
//!
//! **Pod IP config without volume mount:** dùng Kubernetes Downward API qua **environment variables**:
//!
//! ```yaml
//! env:
//! - name: POD_IP
//!   valueFrom:
//!     fieldRef:
//!       fieldPath: status.podIP    # IP của pod (worker)
//! ```
//!
//! **Fallback when no env:** liệt kê interface mạng (get_if_addrs) hoặc UDP connect ra 8.8.8.8.

use std::env;
use std::net::{IpAddr, Ipv4Addr, UdpSocket};

/// Lấy IP local từ interface mạng (không cần kết nối internet, hợp mạng nội bộ).
#[allow(dead_code)]
fn get_local_ip_via_interfaces() -> Option<String> {
    let addrs = get_if_addrs::get_if_addrs().ok()?;
    for iface in addrs {
        let ip = iface.ip();
        if ip.is_loopback() {
            continue;
        }
        if let IpAddr::V4(v4) = ip {
            if !v4.is_unspecified() {
                return Some(v4.to_string());
            }
        }
    }
    None
}

/// Lấy IP local qua UDP: connect ra địa chỉ ngoài (cần route ra internet).
#[allow(dead_code)]
fn get_local_ip_via_udp() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    let ip = addr.ip();
    if ip.is_loopback() || ip == IpAddr::V4(Ipv4Addr::UNSPECIFIED) {
        return None;
    }
    Some(ip.to_string())
}

/// Trả về IP của worker (pod). Ưu tiên env, rồi interface mạng (nội bộ), cuối cùng UDP ra ngoài.
#[allow(dead_code)]
pub fn get_worker_ip() -> Option<String> {
    env::var("POD_IP")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| env::var("HOST_IP").ok().filter(|s| !s.is_empty()))
        .or_else(|| env::var("MY_POD_IP").ok().filter(|s| !s.is_empty()))
        .or_else(get_local_ip_via_interfaces)
        .or_else(get_local_ip_via_udp)
}
