//! Global config: port, DB DSN, timeouts, server limits (BECT-only; no BOO/VDTC/VETC).

use std::env;
use std::sync::{Arc, OnceLock};

pub const DEFAULT_PORT: u16 = 8080;

/// Global config (BECT-only).
#[derive(Debug, Clone)]
pub struct Config {
    pub port_listen: u16,

    // Server connection limits (no max_connections: accept all)
    /// Timeout ép đóng nếu chưa handshake (0 = tắt, không ép đóng; dùng TCP keepalive để phát hiện kết nối half-open).
    pub handshake_timeout_seconds: u64,
    pub socket_read_timeout_seconds: u64, // Timeout for socket read operations (0 = disabled, default: 300 = 5 minutes)
    pub connection_idle_timeout_seconds: u64, // Timeout for idle connections before cleanup (default: 300 = 5 minutes)
    pub dead_connection_check_interval_seconds: u64, // Interval to check for dead connections (default: 30 seconds)

    /// Max concurrent reply-send tasks (router); 0 = unlimited. When at limit, new replies are dropped.
    /// Conservative default to limit impact on main flow (accept, read, process_request) especially on weak servers.
    /// Default when unset: min(500, max(50, parallelism*50)) — e.g. 1 core→50, 4→200, 8→400.
    /// For many concurrent clients (e.g. 500+), set >= expected peak concurrent requests or 0 to avoid dropping replies.
    /// When both this and REPLY_IN_FLIGHT_LIMIT are set, use reply_send_max_concurrent >= reply_in_flight_limit so replies are not dropped at the router solely due to semaphore limit.
    pub reply_send_max_concurrent: usize,
    /// Timeout (seconds) for each reply send to connection queue; 0 = no timeout. On timeout, reply is dropped.
    /// Default when unset: 5 — release permits sooner so main flow is not starved.
    pub reply_send_timeout_secs: u64,

    /// Max request+reply in flight (back-pressure): limit concurrent process_request + send into reply channel; 0 = no limit.
    /// When set, connection tasks acquire a permit before process_request and release after sending reply, so channel never overfills.
    /// Default when unset: 256. Should be >= channel capacity (main.rs) to avoid blocking; use same or larger.
    pub reply_in_flight_limit: usize,

    /// TCP keepalive idle (giây): thời gian không có dữ liệu trước khi gửi probe đầu tiên. 0 = chỉ bật SO_KEEPALIVE, dùng mặc định OS.
    pub tcp_keepalive_idle_seconds: u64,
    /// TCP keepalive interval (giây): khoảng cách giữa các lần probe. Chỉ dùng khi tcp_keepalive_idle_seconds > 0.
    pub tcp_keepalive_interval_seconds: u64,
    /// TCP keepalive retries: số lần probe không phản hồi trước khi đóng (Linux TCP_KEEPCNT). Chỉ dùng khi tcp_keepalive_idle_seconds > 0.
    pub tcp_keepalive_retries: u32,

    // IP block: block client IP after N connection failures (handshake/config/decrypt error), state only in Redis/KeyDB
    pub ip_block_enabled: bool, // Allow IP blocking (default: false; set true to enable)
    pub ip_block_failure_threshold: u32, // Number of failures before block (0 = no block, default: 5)
    pub ip_block_duration_seconds: u64,  // Block duration in seconds (default: 900 = 15 minutes)

    /// Danh sách pattern IP bị từ chối kết nối (denylist). Hỗ trợ: 10.10.10.10, 10.10.*, 10.*. Từ chối ngay, không ghi log.
    pub ip_denylist: Vec<String>,

    /// Bypass thiếu bảng giá/cung đường: khi true, nếu không tìm thấy bảng giá hoặc cung đường thì vẫn cho tính phí 0đ và tiếp tục (không trả NOT_FOUND_PRICE_INFO).
    pub allow_zero_fee_when_no_price_or_route: bool,

    /// Số dư tối thiểu (VNĐ) khi BECT check-in lane IN: nếu > 0 thì kiểm tra available_balance >= giá trị này; không đủ trả 104 (ACCOUNT_NOT_ENOUGH_MONEY). 0 = tắt kiểm tra. Đồng bộ Java (getMinimumBalance / checkinCloseIn).
    pub bect_minimum_balance_checkin: i64,

    /// Khi true: chỉ áp dụng kiểm tra min balance khi check-in lane IN nếu trạm mở (toll_type "O"). Khi false: áp dụng cho mọi lane IN. Java áp dụng hold+min balance chỉ khi stationLane ∈ {OPEN=0, HOLD=6, REFUND=7, ONLY_REFUND=8, CAM=9}. Xem docs/BECT_LANETYPE_MIN_BALANCE.md.
    pub bect_minimum_balance_checkin_open_only: bool,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let _ = dotenvy::dotenv();

        let port_listen = env::var("PORT_LISTEN")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(DEFAULT_PORT);

        // Server connection limits (no max_connections)
        let handshake_timeout_seconds = env::var("HANDSHAKE_TIMEOUT_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0); // Default 0 = disabled: không ép đóng khi client vẫn kết nối; dùng TCP keepalive để phát hiện half-open

        let socket_read_timeout_seconds = env::var("SOCKET_READ_TIMEOUT_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(300); // Default 300 seconds (5 minutes)

        let connection_idle_timeout_seconds = env::var("CONNECTION_IDLE_TIMEOUT_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(300); // Default 300 seconds (5 minutes)

        let dead_connection_check_interval_seconds =
            env::var("DEAD_CONNECTION_CHECK_INTERVAL_SECONDS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(30); // Default 30 seconds

        let reply_send_max_concurrent = env::var("REPLY_SEND_MAX_CONCURRENT")
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or_else(|| {
                // Conservative: limit reply-send concurrency so main flow (accept, read, process_request) keeps CPU.
                std::thread::available_parallelism()
                    .map(|p| (p.get() * 50).min(500).max(50))
                    .unwrap_or(100)
            });

        let reply_send_timeout_secs = env::var("REPLY_SEND_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(2); // 2s default: release permits sooner, reduce impact on main flow

        let reply_in_flight_limit = env::var("REPLY_IN_FLIGHT_LIMIT")
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(256); // back-pressure: max concurrent process_request+send

        // TCP keepalive trên server: phát hiện kết nối half-open (client/router đã ngắt nhưng server chưa biết)
        let tcp_keepalive_idle_seconds = env::var("TCP_KEEPALIVE_IDLE_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(60); // Default 60s idle trước probe đầu tiên
        let tcp_keepalive_interval_seconds = env::var("TCP_KEEPALIVE_INTERVAL_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(10); // Default 10s giữa các probe
        let tcp_keepalive_retries = env::var("TCP_KEEPALIVE_RETRIES")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(3); // Default 3 lần probe (Linux TCP_KEEPCNT)

        let ip_block_enabled = env::var("IP_BLOCK_ENABLED")
            .ok()
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(false); // Default false: disable IP block until explicitly enabled

        let ip_block_failure_threshold = env::var("IP_BLOCK_FAILURE_THRESHOLD")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(5); // Default 5 failures before block

        let ip_block_duration_seconds = env::var("IP_BLOCK_DURATION_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(900); // Default 900 seconds (15 minutes)

        let ip_denylist = env::var("IP_DENYLIST")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let allow_zero_fee_when_no_price_or_route = crate::utils::parse_env_bool_loose(
            env::var("ALLOW_ZERO_FEE_WHEN_NO_PRICE_OR_ROUTE")
                .ok()
                .as_deref(),
        );

        let bect_minimum_balance_checkin = env::var("BECT_MINIMUM_BALANCE_CHECKIN")
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .unwrap_or(0)
            .max(0);

        let bect_minimum_balance_checkin_open_only = crate::utils::parse_env_bool_loose(
            env::var("BECT_MINIMUM_BALANCE_CHECKIN_OPEN_ONLY")
                .ok()
                .as_deref(),
        );

        Ok(Config {
            port_listen,
            handshake_timeout_seconds,
            socket_read_timeout_seconds,
            connection_idle_timeout_seconds,
            dead_connection_check_interval_seconds,
            reply_send_max_concurrent,
            reply_send_timeout_secs,
            reply_in_flight_limit,
            tcp_keepalive_idle_seconds,
            tcp_keepalive_interval_seconds,
            tcp_keepalive_retries,
            ip_block_enabled,
            ip_block_failure_threshold,
            ip_block_duration_seconds,
            ip_denylist,
            allow_zero_fee_when_no_price_or_route,
            bect_minimum_balance_checkin,
            bect_minimum_balance_checkin_open_only,
        })
    }
    // Trả về reference đến config global (lazy load lần đầu)
    pub fn get() -> &'static Config {
        static CONFIG: OnceLock<Config> = OnceLock::new();

        CONFIG.get_or_init(|| {
            Config::load().expect("Không load được config. Kiểm tra biến môi trường!")
        })
    }

    // Nếu bạn muốn clone Arc<Config> để pass vào task (ít dùng hơn)
    #[allow(dead_code)] // Có thể được sử dụng khi cần share config giữa các tasks
    pub fn get_arc() -> Arc<Config> {
        Arc::new(Self::get().clone())
    }

    /// BECT-only: always returns 3 (BECT/ETC). Kept for compatibility with ETDR sync/merge/build and checkin common.
    pub fn get_vehicle_boo_type(_etag: &str) -> i32 {
        3
    }
}
