//! Cấu hình Kafka từ biến môi trường.
//! Nếu KAFKA_BOOTSTRAP_SERVERS không set thì Kafka bị tắt.

use std::env;

const DEFAULT_BOOTSTRAP: &str = "localhost:9092";
/// Topic producer (publish): gửi checkin/checkout lên HUB
const DEFAULT_TOPIC_HUB_TRANS: &str = "topics.hub.trans.online";
/// Topic consumer: nhận bản tin đồng bộ checkin từ HUB về BOO
const DEFAULT_TOPIC_CHECKIN_HUB_ONLINE: &str = "topics.hub.checkin.trans.online";
const DEFAULT_DATA_VERSION: &str = "1.0";

/// Cấu hình Kafka (tùy chọn). Load từ env.
#[derive(Debug, Clone)]
pub struct KafkaConfig {
    /// Bootstrap servers, ví dụ "localhost:9092"
    pub bootstrap_servers: String,
    /// Topic producer: gửi checkin/checkout lên HUB (mặc định DEFAULT_TOPIC_HUB_TRANS)
    pub topic_trans_hub_online: String,
    /// Topic consumer: nhận đồng bộ từ HUB (topics.hub.checkin.trans.online)
    pub topic_checkin_hub_online: String,
    /// Bật Kafka consumer (nhận CHECKIN_HUB_INFO từ HUB). Mặc định true khi Kafka enabled.
    pub consumer_enabled: bool,
    /// Phiên bản dữ liệu event
    pub data_version: String,
    /// Bật Kafka (true nếu KAFKA_BOOTSTRAP_SERVERS được set)
    pub enabled: bool,
    /// Request timeout ms (mặc định 5000)
    pub request_timeout_ms: u32,
    /// Số lần retry khi gửi thất bại (mặc định 10, Kafka offline sẽ retry đến khi thành công hoặc hết số lần)
    pub retries: u32,
    /// Delay (ms) trước retry lần đầu (mặc định 1000)
    pub retry_initial_delay_ms: u64,
    /// Delay tối đa (ms) giữa các lần retry - exponential backoff cap (mặc định 60_000)
    pub retry_max_delay_ms: u64,
    /// Sức chứa hàng đợi Kafka (số event tối đa chờ gửi). Khi đầy, event cũ nhất bị bỏ để nhường chỗ cho event mới (mặc định 10000)
    pub queue_capacity: usize,
    /// Số record tối đa gửi trong một lần produce (batch). Tăng giúp Kafka nhận nhanh hơn khi tải cao (mặc định 50)
    pub send_batch_size: usize,
    /// Chu kỳ kiểm tra kết nối còn sống (giây). 0 = tắt. Nên bật khi Kafka qua nhiều lớp mạng để phát hiện server down/restart (mặc định 60)
    pub liveness_check_interval_secs: u64,
    /// Username cho SASL authentication (tùy chọn, nếu không set thì không dùng authentication)
    pub username: Option<String>,
    /// Password cho SASL authentication (tùy chọn, chỉ dùng khi username được set)
    pub password: Option<String>,
    /// Số partition cố định cho topic consumer (topics.hub.checkin.trans.online). Nếu set thì bỏ qua auto-detect, tránh log ERROR khi probe partition không tồn tại.
    pub topic_checkin_partition_count: Option<i32>,
    /// Số partition cố định cho topic producer (gửi checkin/checkout). Nếu set thì không gọi list_topics, giảm rủi ro timeout/cluster lớn; dùng khi đã biết chắc số partition.
    pub topic_trans_partition_count: Option<i32>,
    /// Chỉ định partition cố định khi produce (0-based). Nếu set thì mọi message gửi vào partition này, bỏ qua chọn theo key. Dùng cho test hoặc routing đặc biệt.
    pub topic_trans_fixed_partition: Option<i32>,
    /// Key KeyDB cho leader lock; chỉ một pod consumer chạy khi giữ lock. Mặc định "kafka_consumer_leader".
    pub consumer_leader_lock_key: String,
    /// TTL (giây) của leader lock. Renew trước khi hết TTL. Mặc định 60.
    pub consumer_leader_lock_ttl_secs: u64,
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            bootstrap_servers: DEFAULT_BOOTSTRAP.to_string(),
            topic_trans_hub_online: DEFAULT_TOPIC_HUB_TRANS.to_string(),
            topic_checkin_hub_online: DEFAULT_TOPIC_CHECKIN_HUB_ONLINE.to_string(),
            consumer_enabled: false,
            data_version: DEFAULT_DATA_VERSION.to_string(),
            enabled: false,
            request_timeout_ms: 5000,
            retries: 10,
            retry_initial_delay_ms: 1000,
            retry_max_delay_ms: 60_000,
            queue_capacity: 10_000,
            send_batch_size: 50,
            liveness_check_interval_secs: 60,
            username: None,
            password: None,
            topic_checkin_partition_count: None,
            topic_trans_partition_count: None,
            topic_trans_fixed_partition: None,
            consumer_leader_lock_key: "kafka_consumer_leader".to_string(),
            consumer_leader_lock_ttl_secs: 60,
        }
    }
}

impl KafkaConfig {
    /// Load từ env. Nếu KAFKA_BOOTSTRAP_SERVERS không có thì enabled = false.
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();
        let bootstrap_servers =
            env::var("KAFKA_BOOTSTRAP_SERVERS").unwrap_or_else(|_| DEFAULT_BOOTSTRAP.to_string());
        let enabled = env::var("KAFKA_BOOTSTRAP_SERVERS").is_ok() && !bootstrap_servers.is_empty();
        // Topic producer: checkin + checkout. Env KAFKA_TOPIC_TRANS_HUB_ONLINE, default DEFAULT_TOPIC_HUB_TRANS
        let topic_trans_hub_online = env::var("KAFKA_TOPIC_TRANS_HUB_ONLINE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_TOPIC_HUB_TRANS.to_string());
        let topic_checkin_hub_online = env::var("KAFKA_TOPIC_CHECKIN_HUB_ONLINE")
            .ok()
            .unwrap_or_else(|| DEFAULT_TOPIC_CHECKIN_HUB_ONLINE.to_string());
        let consumer_enabled = env::var("KAFKA_CONSUMER_ENABLED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(enabled); // default: same as enabled
        let data_version =
            env::var("KAFKA_DATA_VERSION").unwrap_or_else(|_| DEFAULT_DATA_VERSION.to_string());
        let request_timeout_ms = env::var("KAFKA_REQUEST_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5000);
        let retries = env::var("KAFKA_RETRIES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        let retry_initial_delay_ms = env::var("KAFKA_RETRY_INITIAL_DELAY_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1000);
        let retry_max_delay_ms = env::var("KAFKA_RETRY_MAX_DELAY_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60_000);
        let queue_capacity = env::var("KAFKA_QUEUE_CAPACITY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10_000);
        let send_batch_size = env::var("KAFKA_SEND_BATCH_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50);
        let liveness_check_interval_secs = env::var("KAFKA_LIVENESS_CHECK_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        // Username và password cho SASL authentication (tùy chọn)
        let username = env::var("KAFKA_USERNAME").ok().filter(|s| !s.is_empty());
        let password = env::var("KAFKA_PASSWORD").ok().filter(|s| !s.is_empty());
        // Số partition cố định cho topic checkin (tránh auto-detect gây ERROR khi probe partition không tồn tại)
        let topic_checkin_partition_count = env::var("KAFKA_TOPIC_CHECKIN_PARTITION_COUNT")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .filter(|&n| n > 0);
        // Số partition cố định cho topic producer (tránh list_topics trên cluster lớn; nếu set thì không gọi metadata)
        let topic_trans_partition_count = env::var("KAFKA_TOPIC_TRANS_PARTITION_COUNT")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .filter(|&n| n > 0);
        // Partition cố định khi produce (0-based); nếu set thì mọi message gửi vào partition này
        let topic_trans_fixed_partition = env::var("KAFKA_TOPIC_TRANS_FIXED_PARTITION")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .filter(|&n| n >= 0);
        let consumer_leader_lock_key = env::var("KAFKA_CONSUMER_LEADER_LOCK_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "kafka_consumer_leader".to_string());
        let consumer_leader_lock_ttl_secs = env::var("KAFKA_CONSUMER_LEADER_LOCK_TTL_SECS")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(60)
            .max(10);
        Self {
            bootstrap_servers,
            topic_trans_hub_online,
            topic_checkin_hub_online,
            consumer_enabled,
            data_version,
            enabled,
            request_timeout_ms,
            retries,
            retry_initial_delay_ms,
            retry_max_delay_ms,
            queue_capacity,
            send_batch_size,
            liveness_check_interval_secs,
            username,
            password,
            topic_checkin_partition_count,
            topic_trans_partition_count,
            topic_trans_fixed_partition,
            consumer_leader_lock_key,
            consumer_leader_lock_ttl_secs,
        }
    }
}
