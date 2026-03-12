//! Kafka producer service: quản lý kết nối, hàng đợi giới hạn, retry + backoff khi lỗi.
//! Tự động tạo topic nếu chưa có trên Kafka (rskafka - pure Rust).
//! - `get_kafka_producer()`: trả về producer nếu đã init. `send_checkin_from_commit_background` / `send_checkout_from_commit_background`: đẩy event vào queue (không block).
//! - `KafkaProducerService::new`, worker task xử lý queue, `send_raw_with_retry`, reconnect khi timeout/liveness fail.
//!
//! Luồng xử lý (tối ưu khi giao dịch nhiều):
//! 1. Handler gọi get_kafka_producer() rồi send_*_from_commit_background(payload) → push vào hàng đợi (không block, không await).
//! 2. Một worker task cố định lấy message từ hàng đợi → build event → send_* → send_raw_with_retry.
//! 3. Hàng đợi có giới hạn (queue_capacity); khi đầy thì bỏ event cũ nhất, giữ event mới (drop oldest).
//! 4. Push chỉ lock ngắn (Mutex + VecDeque), không tốn tài nguyên thừa.
//!
//! Xử lý kết nối treo (hung connection):
//! - Mọi I/O (partition_client + produce) được bọc bởi tokio::time::timeout(request_timeout_ms).
//! - Nếu Kafka không phản hồi → timeout → retry với exponential backoff.
//! - Sau 3 lần timeout liên tiếp thì reconnect client (tạo kết nối mới) rồi tiếp tục retry.
//!
//! Phát hiện Kafka down/restart (qua nhiều lớp mạng):
//! - Nếu bật KAFKA_LIVENESS_CHECK_INTERVAL_SECS > 0, task nền định kỳ gửi request nhẹ (metadata) tới broker.
//! - Nếu timeout hoặc lỗi → coi như mất kết nối → reconnect ngay, lần produce tiếp theo dùng connection mới.
//!
//! Đồng nhất reconnect (một luồng duy nhất):
//! - Worker (3 timeouts) và liveness (check fail) chỉ gửi yêu cầu reconnect qua channel, không gọi reconnect trực tiếp.
//! - Một task chuyên reconnect_loop nhận yêu cầu và gọi reconnect() tuần tự → tránh check_liveness đang waiting
//!   thì reconnect (timeout) gọi lại; debounce vẫn áp dụng trong reconnect().

use crate::cache::config::keydb::KeyDB;
use crate::configs::kafka::KafkaConfig;
use crate::constants::kafka_producer;
use crate::models::hub_events::{CheckinEvent, CheckinEventData, CheckoutEvent, CheckoutEventData};
use crate::models::ETDR::BOORatingDetail;
use crate::utils::timestamp_ms;

use super::types::{CheckinCommitPayload, CheckoutCommitPayload, KafkaMessage};
use chrono::Utc;
use once_cell::sync::{Lazy, OnceCell};
use parking_lot::Mutex;
use rskafka::client::partition::UnknownTopicHandling;
use rskafka::client::{ClientBuilder, Credentials, SaslConfig};
use rskafka::record::Record;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::sync::Semaphore;
use tokio::time::{sleep, timeout, Duration};
use tracing;

/// Headers rỗng dùng chung, tránh tạo BTreeMap mới mỗi record.
static EMPTY_HEADERS: Lazy<BTreeMap<String, Vec<u8>>> = Lazy::new(BTreeMap::new);

/// Holder toàn cục cho Kafka producer (None nếu không cấu hình).
pub(crate) static KAFKA_PRODUCER: OnceCell<Option<Arc<KafkaProducerService>>> = OnceCell::new();

/// Trả về reference tới Kafka producer nếu đã bật.
pub fn get_kafka_producer() -> Option<Arc<KafkaProducerService>> {
    KAFKA_PRODUCER.get().and_then(|o| o.clone())
}

/// Hàng đợi pending khi Kafka chưa ready: event đẩy vào đây, khi producer ready sẽ drain vào queue của producer.
pub struct PendingKafkaQueue {
    buf: Mutex<VecDeque<KafkaMessage>>,
    cap: usize,
}

impl PendingKafkaQueue {
    fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            buf: Mutex::new(VecDeque::with_capacity(cap)),
            cap,
        }
    }

    /// Đẩy message vào pending queue (drop oldest khi đầy).
    pub(crate) fn push(&self, msg: KafkaMessage) {
        let mut g = self.buf.lock();
        if g.len() >= self.cap {
            let _ = g.pop_front();
        }
        g.push_back(msg);
    }

    /// Lấy toàn bộ message ra (để chuyển sang producer khi ready). Queue trở thành rỗng sau khi gọi.
    pub(crate) fn drain(&self) -> Vec<KafkaMessage> {
        let mut g = self.buf.lock();
        g.drain(..).collect()
    }
}

static PENDING_KAFKA_QUEUE: OnceCell<Arc<PendingKafkaQueue>> = OnceCell::new();

/// Khởi tạo pending queue khi Kafka enabled (gọi từ main trước spawn init). Trả về Arc để truyền vào init task.
pub fn init_pending_queue(capacity: usize) -> Arc<PendingKafkaQueue> {
    let q = Arc::new(PendingKafkaQueue::new(capacity));
    let _ = PENDING_KAFKA_QUEUE.set(Arc::clone(&q));
    q
}

/// Trả về pending queue nếu đã init (Kafka enabled nhưng producer chưa ready).
pub fn get_pending_kafka_queue() -> Option<Arc<PendingKafkaQueue>> {
    PENDING_KAFKA_QUEUE.get().cloned()
}

/// Đẩy event checkin vào pending queue khi producer chưa ready (handler gọi).
/// Nếu pending queue không tồn tại (Kafka disabled) thì event bị bỏ qua và log debug.
pub fn push_pending_checkin(payload: CheckinCommitPayload) {
    if let Some(q) = get_pending_kafka_queue() {
        tracing::info!(
            request_id = payload.request_id,
            etag = %payload.etag,
            "[Kafka] event queued checkin (pending until producer ready)"
        );
        q.push(KafkaMessage::Checkin(payload));
    } else {
        tracing::debug!(
            request_id = payload.request_id,
            etag = %payload.etag,
            "[Kafka] event skipped (Kafka disabled, set KAFKA_BOOTSTRAP_SERVERS to enable)"
        );
    }
}

/// Đẩy event checkout vào pending queue khi producer chưa ready (handler gọi).
/// Nếu pending queue không tồn tại (Kafka disabled) thì event bị bỏ qua và log debug.
pub fn push_pending_checkout(payload: CheckoutCommitPayload) {
    if let Some(q) = get_pending_kafka_queue() {
        tracing::info!(
            request_id = payload.request_id,
            etag = %payload.etag,
            "[Kafka] event queued checkout (pending until producer ready)"
        );
        q.push(KafkaMessage::Checkout(payload));
    } else {
        tracing::debug!(
            request_id = payload.request_id,
            etag = %payload.etag,
            "[Kafka] event skipped (Kafka disabled, set KAFKA_BOOTSTRAP_SERVERS to enable)"
        );
    }
}

/// Chuyển toàn bộ message từ pending queue sang producer (gọi sau khi KAFKA_PRODUCER.set).
/// Không gây double message: mỗi event chỉ vào pending HOẶC producer (send_*_to_kafka chỉ rẽ một nhánh);
/// drain() xóa hết khỏi pending rồi push vào producer queue, mỗi message chỉ được gửi một lần.
pub fn drain_pending_into_producer(
    pending: Arc<PendingKafkaQueue>,
    producer: Arc<KafkaProducerService>,
) {
    let messages = pending.drain();
    let count = messages.len();
    if count == 0 {
        return;
    }
    producer.push_messages(messages);
    tracing::info!(count, "[Kafka] drained pending queue into producer");
}

/// Hàng đợi giới hạn: khi đầy thì bỏ event cũ nhất (drop oldest), luôn nhận event mới.
/// Dùng parking_lot::Mutex (nhanh hơn std khi contention) + preallocate đủ capacity để tăng tốc push khi tải lớn.
struct DropOldestQueue {
    buf: Mutex<VecDeque<KafkaMessage>>,
    cap: usize,
    sem: Semaphore,
}

impl DropOldestQueue {
    /// Số permit còn lại (để debug).
    fn available_permits(&self) -> usize {
        self.sem.available_permits()
    }
}

impl DropOldestQueue {
    fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            buf: Mutex::new(VecDeque::with_capacity(cap)),
            cap,
            sem: Semaphore::new(0),
        }
    }

    /// Đẩy message vào queue (không block). Nếu đầy thì bỏ message cũ nhất. Tối ưu cho số lượng lớn / queue đầy.
    ///
    /// LOGIC QUAN TRỌNG:
    /// - Khi queue chưa đầy: Thêm message mới + tăng permit mới
    /// - Khi queue đầy: Drop message cũ + thêm message mới + tăng permit mới
    ///   Lý do: Message cũ có thể đã được pop và permit đã bị consume.
    ///   Nếu không tăng permit mới, message mới sẽ không có permit tương ứng.
    ///   Nếu message cũ chưa được pop, permit của nó vẫn còn nhưng message đã bị drop.
    ///   Trong cả hai trường hợp, chúng ta cần permit mới cho message mới.
    #[inline(always)]
    fn push(&self, msg: KafkaMessage) {
        let (was_full, dropped_kind, buffer_len_after) = {
            let mut g = self.buf.lock();
            let was_full = g.len() >= self.cap;
            let dropped = if was_full {
                g.pop_front().map(|m| match &m {
                    KafkaMessage::Checkin(_) => "checkin",
                    KafkaMessage::Checkout(_) => "checkout",
                })
            } else {
                None
            };
            g.push_back(msg);
            let len = g.len();
            (was_full, dropped, len)
        };

        if was_full {
            tracing::warn!(
                dropped = dropped_kind.unwrap_or("unknown"),
                buffer_len = buffer_len_after,
                capacity = self.cap,
                "[Kafka] queue full, dropped oldest event (producer may be slow or stuck)"
            );
        }

        // LUÔN tăng permit cho message mới, bất kể queue có đầy hay không
        // Điều này đảm bảo mỗi message mới luôn có permit tương ứng
        self.sem.add_permits(1);

        let available_permits = self.sem.available_permits();
        tracing::debug!(
            was_full,
            buffer_len = buffer_len_after,
            available_permits,
            "[Kafka] queue push"
        );
        if available_permits > buffer_len_after {
            tracing::warn!(
                available_permits,
                buffer_len = buffer_len_after,
                "[Kafka] queue inconsistency: available_permits > buffer_len"
            );
        }
    }

    /// Lấy message (block cho đến khi có). Worker gọi.
    /// Trả về None chỉ khi semaphore bị đóng (process đang shutdown).
    ///
    /// LOGIC:
    /// 1. Acquire permit từ semaphore (chờ đến khi có permit)
    /// 2. Lock buffer và lấy message ngay lập tức
    /// 3. Nếu buffer rỗng (race condition với try_pop), drop permit và retry
    /// 4. Nếu có message, trả về message và drop permit
    ///
    /// Đảm bảo: Mỗi permit tương ứng với một message trong buffer.
    /// Nếu có permit nhưng buffer rỗng, đó là race condition và sẽ retry.
    async fn pop(&self) -> Option<KafkaMessage> {
        loop {
            // Bước 1: Chờ permit từ semaphore
            // acquire().await sẽ block cho đến khi có permit available
            // hoặc semaphore bị đóng
            let permit = match self.sem.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(error = ?e, "[Kafka] queue semaphore closed (shutdown)");
                    return None;
                }
            };

            // Bước 2: Lock buffer và lấy message ngay lập tức
            // Phải lock ngay sau khi acquire permit để giảm thiểu race condition
            let msg = {
                let mut g = self.buf.lock();
                let buf_len_before = g.len();
                let msg = g.pop_front();
                let buf_len_after = g.len();

                if msg.is_none() {
                    // Race condition: try_pop() đã lấy message trước khi pop() lock buffer
                    // Hoặc có bug trong logic push() khi queue đầy
                    //
                    // Tình huống có thể xảy ra:
                    // 1. pop() acquire permit (có 1 permit, buffer có 1 message)
                    // 2. try_pop() được gọi từ worker loop, try_acquire thành công
                    // 3. try_pop() lock buffer và pop message (buffer rỗng)
                    // 4. pop() lock buffer nhưng buffer đã rỗng
                    //
                    // Giải pháp: Drop permit và retry bằng cách tiếp tục loop
                    // Drop permit để tránh leak và retry
                    drop(permit);
                    continue;
                }

                tracing::debug!(buf_len_before, buf_len_after, "[Kafka] queue pop ok");
                msg
            };

            // Bước 3: Drop permit và trả về message
            // Permit đã được consume đúng cách (1 permit = 1 message)
            drop(permit);

            return msg;
        }
    }

    /// Lấy message không chờ (trả None nếu hàng đợi rỗng). Để worker drain batch nhanh.
    ///
    /// LOGIC:
    /// 1. Try acquire permit (không block)
    /// 2. Nếu thành công, lock buffer và lấy message
    /// 3. Nếu buffer rỗng (không nên xảy ra), drop permit và trả về None
    ///
    /// LƯU Ý: try_pop() được gọi từ worker loop sau khi pop() đã lấy message đầu tiên.
    /// Có thể có race condition nếu nhiều try_pop() được gọi đồng thời.
    #[inline(always)]
    fn try_pop(&self) -> Option<KafkaMessage> {
        // Bước 1: Try acquire permit (không block)
        let permit = match self.sem.try_acquire() {
            Ok(p) => p,
            Err(_) => {
                // Không có permit available → không có message trong buffer
                return None;
            }
        };

        // Bước 2: Lock buffer và lấy message ngay lập tức
        let msg = {
            let mut g = self.buf.lock();
            let msg = g.pop_front();

            if msg.is_none() {
                // Race condition: Có permit nhưng buffer rỗng
                // Drop permit để tránh leak
                drop(permit);
                return None;
            }

            msg
        };

        // Bước 3: Drop permit và trả về message
        drop(permit);
        msg
    }
}

/// Service gửi event checkin/checkout lên Kafka (rskafka client).
/// Dùng hàng đợi giới hạn + một worker; khi đầy thì bỏ event cũ nhất, giữ event mới.
/// Client được giữ trong Arc<RwLock<>> để có thể reconnect (thay client mới) sau 3 lần timeout liên tiếp.
/// reconnect_tx: worker và liveness chỉ gửi () vào đây; một task reconnect_loop duy nhất gọi reconnect().
/// last_reconnect: debounce trong reconnect() để tránh reconnect quá dày.
/// keydb: khi Some, cache batch gửi thất bại vào list KeyDB và có task replay khi online.
pub struct KafkaProducerService {
    client: Arc<RwLock<rskafka::client::Client>>,
    reconnect_tx: mpsc::UnboundedSender<()>,
    last_reconnect: Mutex<Option<Instant>>,
    /// Cache số partition của topic producer; clear khi reconnect để refetch.
    partition_count_cache: parking_lot::RwLock<Option<i32>>,
    config: KafkaConfig,
    data_version: Arc<str>,
    topic_trans_hub_online: Arc<str>,
    queue: Arc<DropOldestQueue>,
    keydb: Option<Arc<KeyDB>>,
}

impl KafkaProducerService {
    /// Kiểm tra TCP connection đến các Kafka brokers (giống telnet) để xác định network có thông hay không.
    /// Không chỉ connect mà còn verify có thể gửi/nhận data.
    /// Trả về true nếu ít nhất một broker reachable, false nếu tất cả đều không reachable.
    pub async fn verify_kafka_brokers(bootstrap_servers: &str, timeout_secs: u64) -> bool {
        let brokers: Vec<String> = bootstrap_servers
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        if brokers.is_empty() {
            tracing::warn!("[Kafka] verify_brokers: no brokers in bootstrap_servers");
            return false;
        }
        tracing::debug!(
            broker_count = brokers.len(),
            "[Kafka] verifying TCP to brokers"
        );

        let mut reachable_count = 0;
        let mut unreachable_count = 0;

        for broker in &brokers {
            // Test TCP connection giống telnet: connect + verify có thể gửi/nhận data
            let check_result = timeout(
                Duration::from_secs(timeout_secs),
                Self::test_tcp_connection_telnet_style(broker),
            )
            .await;

            match check_result {
                Ok(Ok(true)) => {
                    tracing::debug!(broker = %broker, "[Kafka] broker reachable");
                    reachable_count += 1;
                }
                Ok(Ok(false)) => {
                    tracing::warn!(broker = %broker, "[Kafka] broker connected but test failed");
                    unreachable_count += 1;
                }
                Ok(Err(e)) => {
                    tracing::warn!(broker = %broker, error = %e, "[Kafka] broker unreachable");
                    unreachable_count += 1;
                }
                Err(_) => {
                    tracing::warn!(broker = %broker, timeout_secs, "[Kafka] broker timeout");
                    unreachable_count += 1;
                }
            }
        }

        if reachable_count > 0 {
            tracing::info!(
                reachable = reachable_count,
                total = brokers.len(),
                unreachable = unreachable_count,
                "[Kafka] network check ok (reachable/total)"
            );
            true
        } else {
            tracing::error!(
                total = brokers.len(),
                "[Kafka] network check failed: all brokers unreachable (check firewall, telnet host port)"
            );
            false
        }
    }

    /// Test TCP connection giống telnet: connect và verify có thể gửi/nhận data.
    /// Trả về Ok(true) nếu connection OK và có thể gửi/nhận data, Ok(false) nếu connect được nhưng test fail, Err nếu không connect được.
    async fn test_tcp_connection_telnet_style(
        broker: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Bước 1: Connect TCP
        let mut stream = TcpStream::connect(broker).await?;

        // Bước 2: Verify connection có thể write (gửi data)
        // Kafka server sẽ không phản hồi ngay khi connect, nhưng connection phải có thể write
        // Thử gửi một số bytes nhỏ để verify connection thực sự hoạt động
        let test_data = [0u8; 0]; // Empty data, chỉ để test write capability
        match stream.write(&test_data).await {
            Ok(_) => {
                // Bước 3: Verify connection có thể read (nhận data)
                // Set read timeout ngắn để không block lâu
                // Kafka có thể không gửi data ngay, nhưng connection phải ready để read
                let mut buffer = [0u8; 1];
                match timeout(Duration::from_millis(100), stream.read(&mut buffer)).await {
                    Ok(Ok(_)) => {
                        // Có data hoặc connection closed (EOF) - đều OK, nghĩa là connection hoạt động
                        Ok(true)
                    }
                    Ok(Err(e)) => {
                        // Read error - nhưng connection đã established nên vẫn OK
                        tracing::debug!(broker = %broker, error = %e, "[Kafka] read test (connection OK)");
                        Ok(true)
                    }
                    Err(_) => {
                        // Timeout - không sao, Kafka có thể không gửi data ngay
                        // Connection đã established và có thể write, nên vẫn OK
                        Ok(true)
                    }
                }
            }
            Err(e) => {
                // Không thể write - connection có vấn đề
                Err(format!("Connection established but cannot write: {}", e).into())
            }
        }
    }

    /// Phân loại lỗi Kafka để xác định nguyên nhân (network vs authentication).
    fn classify_kafka_error(error: &dyn std::error::Error) -> (&'static str, bool) {
        let error_msg = error.to_string().to_lowercase();

        // Kiểm tra các từ khóa liên quan đến authentication
        let auth_keywords = [
            "authentication",
            "auth",
            "unauthorized",
            "invalid credentials",
            "invalid username",
            "invalid password",
            "sasl",
            "login",
            "credential",
            "access denied",
            "permission denied",
        ];

        let is_auth_error = auth_keywords
            .iter()
            .any(|keyword| error_msg.contains(keyword));

        if is_auth_error {
            ("authentication", true)
        } else {
            // Kiểm tra các từ khóa liên quan đến network/connection
            let network_keywords = [
                "timeout",
                "connection",
                "unreachable",
                "refused",
                "network",
                "dns",
                "resolve",
                "no route to host",
                "connection reset",
            ];

            let is_network_error = network_keywords
                .iter()
                .any(|keyword| error_msg.contains(keyword));

            if is_network_error {
                ("network", false)
            } else {
                ("unknown", false)
            }
        }
    }

    /// Tạo Kafka client với authentication nếu có username/password.
    async fn build_client(
        config: &KafkaConfig,
    ) -> Result<rskafka::client::Client, Box<dyn std::error::Error + Send + Sync>> {
        let brokers: Vec<String> = config
            .bootstrap_servers
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        let mut builder = ClientBuilder::new(brokers.clone());

        // Nếu có username và password, sử dụng SASL authentication
        let has_auth =
            if let (Some(username), Some(password)) = (&config.username, &config.password) {
                let credentials = Credentials {
                    username: username.clone(),
                    password: password.clone(),
                };
                let sasl_config = SaslConfig::Plain(credentials);
                builder = builder.sasl_config(sasl_config);
                tracing::info!(username = %username, "[Kafka] SASL auth enabled");
                true
            } else {
                false
            };

        builder.build().await.map_err(|e| {
            let error_msg = e.to_string();
            let error_type = Self::classify_kafka_error(&e);

            if error_type.1 {
                tracing::error!(
                    brokers = %brokers.join(", "),
                    error = %error_msg,
                    hint = if has_auth { "check KAFKA_USERNAME/KAFKA_PASSWORD" } else { "server requires auth, credentials not provided" },
                    "[Kafka] auth failed"
                );
            } else if error_type.0 == "network" {
                tracing::warn!(
                    brokers = %brokers.join(", "),
                    error = %error_msg,
                    "[Kafka] network/connection error (check brokers reachable)"
                );
            } else {
                tracing::warn!(
                    brokers = %brokers.join(", "),
                    error = %error_msg,
                    "[Kafka] connection error"
                );
            }

            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })
    }

    /// Khởi tạo client, hàng đợi và worker; trả về Arc để handler dùng chung.
    /// Nếu keydb.is_some(): khi gửi batch thất bại sau retry sẽ cache payload vào KeyDB list và có task replay khi online.
    pub async fn new_async(
        config: KafkaConfig,
        keydb: Option<Arc<KeyDB>>,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        let client = Self::build_client(&config).await?;
        Self::ensure_topics_async(&client, &config).await?;
        let client = Arc::new(RwLock::new(client));
        let data_version = Arc::from(config.data_version.as_str());
        let topic_trans_hub_online = Arc::from(config.topic_trans_hub_online.as_str());
        let capacity = config.queue_capacity.max(1);
        let queue = Arc::new(DropOldestQueue::new(capacity));
        let liveness_interval_secs = config.liveness_check_interval_secs;
        let (reconnect_tx, mut reconnect_rx) = mpsc::unbounded_channel();
        let service = Arc::new(Self {
            client,
            reconnect_tx,
            last_reconnect: Mutex::new(None),
            partition_count_cache: parking_lot::RwLock::new(None),
            config,
            data_version,
            topic_trans_hub_online,
            queue: Arc::clone(&queue),
            keydb,
        });
        tokio::spawn(Self::reconnect_loop(Arc::clone(&service), reconnect_rx));
        tokio::spawn(Self::worker_loop(Arc::clone(&service), queue));
        if liveness_interval_secs > 0 {
            tokio::spawn(Self::liveness_loop(
                Arc::clone(&service),
                liveness_interval_secs,
            ));
            tracing::info!(
                interval_secs = liveness_interval_secs,
                "[Kafka] liveness check enabled"
            );
        }
        if service.keydb.is_some() {
            tokio::spawn(Self::keydb_replay_loop(Arc::clone(&service)));
            tracing::info!(
                "[Kafka] KeyDB failover enabled (failed batches cached, replayed when online)"
            );
        }
        Ok(service)
    }

    /// Task duy nhất xử lý reconnect: nhận yêu cầu từ worker/liveness, gọi reconnect() tuần tự (có debounce).
    async fn reconnect_loop(service: Arc<Self>, mut rx: mpsc::UnboundedReceiver<()>) {
        while let Some(()) = rx.recv().await {
            if let Err(e) = service.reconnect().await {
                tracing::error!(error = %e, "[Kafka] reconnect failed");
            }
        }
    }

    /// Yêu cầu reconnect (không block). Worker và liveness gọi method này; reconnect_loop sẽ thực hiện.
    fn request_reconnect(&self) {
        if self.reconnect_tx.send(()).is_err() {
            tracing::debug!("[Kafka] reconnect channel closed");
        }
    }

    /// Vòng lặp định kỳ kiểm tra Kafka còn sống; nếu không (timeout/lỗi) thì gửi yêu cầu reconnect (đồng nhất với worker).
    async fn liveness_loop(service: Arc<Self>, interval_secs: u64) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
        interval.tick().await; // first tick fires immediately, skip it
        loop {
            interval.tick().await;
            if !service.check_liveness().await {
                tracing::info!("[Kafka] liveness down, requesting reconnect");
                service.request_reconnect();
            }
        }
    }

    /// Cache checkin payloads vào KeyDB list khi gửi batch thất bại; replay task sẽ lấy ra và gửi lại khi online.
    /// Giới hạn độ dài list + TTL để tránh lãng phí tài nguyên / leak. Persist một lần per batch (rpush_many) để tối ưu round-trip.
    async fn persist_checkin_payloads_to_keydb(keydb: &KeyDB, payloads: &[CheckinCommitPayload]) {
        let mut jsons: Vec<String> = Vec::with_capacity(payloads.len());
        for p in payloads {
            match serde_json::to_string(p) {
                Ok(json) => jsons.push(json),
                Err(e) => {
                    tracing::error!(request_id = p.request_id, error = %e, "[Kafka] checkin payload serialize for KeyDB failed");
                }
            }
        }
        if jsons.is_empty() {
            return;
        }
        if let Err(e) = keydb
            .rpush_many_trim_ttl(
                kafka_producer::KEYDB_LIST_CHECKIN,
                &jsons,
                kafka_producer::KEYDB_PENDING_MAX_LIST_LEN,
                kafka_producer::KEYDB_PENDING_TTL_SECS,
            )
            .await
        {
            tracing::error!(count = jsons.len(), error = %e, "[Kafka] KeyDB rpush checkin batch failed");
        } else {
            tracing::info!(
                count = jsons.len(),
                "[Kafka] checkin payloads cached to KeyDB for replay"
            );
        }
    }

    /// Cache checkout payloads vào KeyDB list khi gửi batch thất bại; replay task sẽ lấy ra và gửi lại khi online.
    /// Giới hạn độ dài list + TTL để tránh lãng phí tài nguyên / leak. Persist một lần per batch để tối ưu round-trip.
    async fn persist_checkout_payloads_to_keydb(keydb: &KeyDB, payloads: &[CheckoutCommitPayload]) {
        let mut jsons: Vec<String> = Vec::with_capacity(payloads.len());
        for p in payloads {
            match serde_json::to_string(p) {
                Ok(json) => jsons.push(json),
                Err(e) => {
                    tracing::error!(request_id = p.request_id, error = %e, "[Kafka] checkout payload serialize for KeyDB failed");
                }
            }
        }
        if jsons.is_empty() {
            return;
        }
        if let Err(e) = keydb
            .rpush_many_trim_ttl(
                kafka_producer::KEYDB_LIST_CHECKOUT,
                &jsons,
                kafka_producer::KEYDB_PENDING_MAX_LIST_LEN,
                kafka_producer::KEYDB_PENDING_TTL_SECS,
            )
            .await
        {
            tracing::error!(count = jsons.len(), error = %e, "[Kafka] KeyDB rpush checkout batch failed");
        } else {
            tracing::info!(
                count = jsons.len(),
                "[Kafka] checkout payloads cached to KeyDB for replay"
            );
        }
    }

    /// Vòng lặp định kỳ: lấy payload từ KeyDB list (LPOP batch), đẩy vào queue producer để gửi lại.
    /// Khi gửi thành công (worker xử lý) thì đã xóa khỏi list lúc LPOP. Giới hạn số lượng mỗi chu kỳ để tránh tràn queue.
    async fn keydb_replay_loop(service: Arc<Self>) {
        let keydb = match &service.keydb {
            Some(k) => Arc::clone(k),
            None => return,
        };
        let batch_size =
            kafka_producer::KEYDB_REPLAY_LPOP_BATCH.min(kafka_producer::KEYDB_REPLAY_MAX_PER_CYCLE);
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
            kafka_producer::KEYDB_REPLAY_INTERVAL_SECS,
        ));
        interval.tick().await;
        loop {
            interval.tick().await;
            if !keydb.is_connected().await {
                continue;
            }
            let mut replayed_checkin = 0usize;
            let mut replayed_checkout = 0usize;
            // Replay checkin: lấy theo batch, tối đa kafka_producer::KEYDB_REPLAY_MAX_PER_CYCLE mỗi chu kỳ
            while replayed_checkin < kafka_producer::KEYDB_REPLAY_MAX_PER_CYCLE {
                let take =
                    (kafka_producer::KEYDB_REPLAY_MAX_PER_CYCLE - replayed_checkin).min(batch_size);
                match keydb.lpop_n(kafka_producer::KEYDB_LIST_CHECKIN, take).await {
                    Ok(items) if items.is_empty() => break,
                    Ok(items) => {
                        let mut messages = Vec::with_capacity(items.len());
                        for json in items {
                            match serde_json::from_str::<CheckinCommitPayload>(&json) {
                                Ok(payload) => messages.push(KafkaMessage::Checkin(payload)),
                                Err(e) => {
                                    tracing::error!(error = %e, "[Kafka] KeyDB checkin payload deserialize failed, dropped");
                                }
                            }
                        }
                        let n = messages.len();
                        if n > 0 {
                            service.push_messages(messages);
                            replayed_checkin += n;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "[Kafka] KeyDB lpop_n checkin failed");
                        break;
                    }
                }
            }
            // Replay checkout
            while replayed_checkout < kafka_producer::KEYDB_REPLAY_MAX_PER_CYCLE {
                let take = (kafka_producer::KEYDB_REPLAY_MAX_PER_CYCLE - replayed_checkout)
                    .min(batch_size);
                match keydb
                    .lpop_n(kafka_producer::KEYDB_LIST_CHECKOUT, take)
                    .await
                {
                    Ok(items) if items.is_empty() => break,
                    Ok(items) => {
                        let mut messages = Vec::with_capacity(items.len());
                        for json in items {
                            match serde_json::from_str::<CheckoutCommitPayload>(&json) {
                                Ok(payload) => messages.push(KafkaMessage::Checkout(payload)),
                                Err(e) => {
                                    tracing::error!(error = %e, "[Kafka] KeyDB checkout payload deserialize failed, dropped");
                                }
                            }
                        }
                        let n = messages.len();
                        if n > 0 {
                            service.push_messages(messages);
                            replayed_checkout += n;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "[Kafka] KeyDB lpop_n checkout failed");
                        break;
                    }
                }
            }
            if replayed_checkin > 0 || replayed_checkout > 0 {
                tracing::debug!(
                    checkin = replayed_checkin,
                    checkout = replayed_checkout,
                    "[Kafka] KeyDB replay cycle"
                );
            }
        }
    }

    /// Kiểm tra nhanh kết nối còn sống (gọi metadata/partition; timeout theo request_timeout_ms).
    /// Trả về true nếu OK, false nếu timeout hoặc lỗi (server down / restart / mạng đứt).
    async fn check_liveness(&self) -> bool {
        let timeout_dur = Duration::from_millis(self.config.request_timeout_ms as u64);
        let client_lock = Arc::clone(&self.client);
        let topic = self.topic_trans_hub_online.to_string();
        match timeout(timeout_dur, async move {
            let guard = client_lock.read().await;
            guard
                .partition_client(topic, 0, UnknownTopicHandling::Retry)
                .await
                .map(|_| ())
        })
        .await
        {
            Ok(Ok(())) => true,
            Ok(Err(e)) => {
                tracing::debug!(error = %e, "[Kafka] liveness check failed");
                false
            }
            Err(_) => {
                tracing::warn!(
                    timeout_ms = self.config.request_timeout_ms,
                    "[Kafka] liveness timeout (server down?)"
                );
                false
            }
        }
    }

    /// Worker: drain batch → build records theo topic → gửi một lần per topic (batch produce).
    /// Batch không đủ batch_size vẫn gửi ngay: chờ có ít nhất 1 msg (pop.await), lấy thêm bằng try_pop
    /// đến khi đủ batch_size hoặc queue rỗng (None) → gửi luôn, không chờ thêm.
    /// Khi gửi thất bại sau retry và keydb.is_some(): cache payload vào KeyDB list để replay khi online.
    async fn worker_loop(service: Arc<Self>, queue: Arc<DropOldestQueue>) {
        let batch_size = service.config.send_batch_size.max(1).min(500);
        let mut batch = Vec::with_capacity(batch_size);
        let mut checkin_records: Vec<(Option<String>, Vec<u8>)> = Vec::with_capacity(batch_size);
        let mut checkout_records: Vec<(Option<String>, Vec<u8>)> = Vec::with_capacity(batch_size);
        let mut checkin_payloads: Vec<CheckinCommitPayload> = Vec::with_capacity(batch_size);
        let mut checkout_payloads: Vec<CheckoutCommitPayload> = Vec::with_capacity(batch_size);
        loop {
            batch.clear();
            match queue.pop().await {
                None => {
                    tracing::warn!("[Kafka] worker loop exit: queue pop None (semaphore closed)");
                    break;
                }
                Some(first) => {
                    batch.push(first);
                    while batch.len() < batch_size {
                        match queue.try_pop() {
                            Some(m) => batch.push(m),
                            None => break,
                        }
                    }
                }
            }
            let checkin_count = batch
                .iter()
                .filter(|m| matches!(m, KafkaMessage::Checkin(_)))
                .count();
            let checkout_count = batch
                .iter()
                .filter(|m| matches!(m, KafkaMessage::Checkout(_)))
                .count();
            tracing::info!(
                batch_size = batch.len(),
                checkin = checkin_count,
                checkout = checkout_count,
                "[Kafka] worker batch"
            );
            checkin_records.clear();
            checkout_records.clear();
            checkin_payloads.clear();
            checkout_payloads.clear();
            for msg in batch.drain(..) {
                match msg {
                    KafkaMessage::Checkin(p) => {
                        let request_id = p.request_id;
                        let etag = p.etag.clone();
                        if let Some((id, bytes)) = service.build_checkin_record(&p) {
                            tracing::debug!(request_id, etag = %etag, bytes_len = bytes.len(), "[Kafka] checkin built");
                            checkin_records.push((Some(id), bytes));
                            checkin_payloads.push(p);
                        } else {
                            tracing::error!(request_id, etag = %etag, "[Kafka] checkin serialize failed, skipped");
                        }
                    }
                    KafkaMessage::Checkout(p) => {
                        let request_id = p.request_id;
                        let etag = p.etag.clone();
                        let rating_detail_count = p.rating_detail.len();
                        if let Some((id, bytes)) = service.build_checkout_record(&p) {
                            tracing::debug!(request_id, etag = %etag, bytes_len = bytes.len(), "[Kafka] checkout built");
                            checkout_records.push((Some(id), bytes));
                            checkout_payloads.push(p);
                        } else {
                            tracing::error!(request_id, etag = %etag, rating_detail_count, "[Kafka] checkout serialize failed, skipped");
                        }
                    }
                }
            }
            if checkin_count > 0 && checkin_records.is_empty() {
                tracing::warn!(
                    checkin_count,
                    "[Kafka] checkin batch empty after build (serialization may have failed)"
                );
            }
            if !checkin_records.is_empty() {
                let n = checkin_records.len();
                let topic = service.topic_trans_hub_online.as_ref();
                tracing::info!(topic, records = n, "[Kafka] sending checkin batch");
                let send_result = timeout(
                    Duration::from_secs(kafka_producer::BATCH_SEND_TOTAL_TIMEOUT_SECS),
                    service.send_raw_batch_with_retry(topic, &checkin_records),
                )
                .await;
                match send_result {
                    Ok(Ok(())) => {
                        tracing::info!(records = n, "[Kafka] checkin batch sent");
                    }
                    Ok(Err(e)) => {
                        let hint = if e.contains("UnknownTopic") || e.contains("not present") {
                            "topic missing? create topic or check KAFKA_TOPIC_TRANS_HUB_ONLINE"
                        } else if e.contains("auth")
                            || e.contains("Authorization")
                            || e.contains("Permission")
                        {
                            "check broker ACL: producer needs write on topic"
                        } else {
                            ""
                        };
                        tracing::error!(
                            topic,
                            records = n,
                            error = %e,
                            hint = %hint,
                            "[Kafka] checkin batch failed"
                        );
                        if let Some(keydb) = &service.keydb {
                            Self::persist_checkin_payloads_to_keydb(keydb, &checkin_payloads).await;
                        }
                    }
                    Err(_) => {
                        tracing::error!(
                            topic,
                            records = n,
                            timeout_secs = kafka_producer::BATCH_SEND_TOTAL_TIMEOUT_SECS,
                            "[Kafka] checkin batch total timeout (worker may have been stuck), requesting reconnect"
                        );
                        service.request_reconnect();
                        if let Some(keydb) = &service.keydb {
                            Self::persist_checkin_payloads_to_keydb(keydb, &checkin_payloads).await;
                        }
                    }
                }
            }
            if !checkout_records.is_empty() {
                let n = checkout_records.len();
                let topic = service.topic_trans_hub_online.as_ref();
                tracing::info!(topic, records = n, "[Kafka] sending checkout batch");
                let send_result = timeout(
                    Duration::from_secs(kafka_producer::BATCH_SEND_TOTAL_TIMEOUT_SECS),
                    service.send_raw_batch_with_retry(topic, &checkout_records),
                )
                .await;
                match send_result {
                    Ok(Ok(())) => {
                        tracing::info!(records = n, "[Kafka] checkout batch sent");
                    }
                    Ok(Err(e)) => {
                        let hint = if e.contains("UnknownTopic") || e.contains("not present") {
                            "topic missing? create topic or check KAFKA_TOPIC_TRANS_HUB_ONLINE"
                        } else if e.contains("auth")
                            || e.contains("Authorization")
                            || e.contains("Permission")
                        {
                            "check broker ACL: producer needs write on topic"
                        } else {
                            ""
                        };
                        tracing::error!(
                            topic,
                            records = n,
                            error = %e,
                            hint = %hint,
                            "[Kafka] checkout batch failed"
                        );
                        if let Some(keydb) = &service.keydb {
                            Self::persist_checkout_payloads_to_keydb(keydb, &checkout_payloads)
                                .await;
                        }
                    }
                    Err(_) => {
                        tracing::error!(
                            topic,
                            records = n,
                            timeout_secs = kafka_producer::BATCH_SEND_TOTAL_TIMEOUT_SECS,
                            "[Kafka] checkout batch total timeout (worker may have been stuck), requesting reconnect"
                        );
                        service.request_reconnect();
                        if let Some(keydb) = &service.keydb {
                            Self::persist_checkout_payloads_to_keydb(keydb, &checkout_payloads)
                                .await;
                        }
                    }
                }
            } else if checkout_count > 0 {
                tracing::warn!(
                    checkout_count,
                    "[Kafka] checkout batch empty after build (serialization may have failed)"
                );
            }
        }
        tracing::warn!("[Kafka] worker loop ended");
    }

    /// Tạo topic nếu chưa tồn tại (bỏ qua lỗi đã tồn tại).
    async fn ensure_topics_async(
        client: &rskafka::client::Client,
        config: &KafkaConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let controller = match client.controller_client() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "[Kafka] controller_client failed (topics may be created on first produce)");
                return Ok(());
            }
        };
        for name in [&config.topic_trans_hub_online] {
            match controller
                .create_topic(name, 1, 1, config.request_timeout_ms as i32)
                .await
            {
                Ok(_) => tracing::info!(topic = %name, "[Kafka] topic created"),
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("already exists") || msg.contains("TOPIC_ALREADY_EXISTS") {
                        tracing::debug!(topic = %name, "[Kafka] topic exists");
                    } else {
                        tracing::warn!(topic = %name, error = %e, "[Kafka] create topic failed (will retry on produce)");
                    }
                }
            }
        }
        Ok(())
    }

    /// Tạo lại kết nối Kafka (gọi sau vài lần timeout liên tiếp hoặc khi liveness check fail).
    /// Debounce: nếu vừa reconnect trong kafka_producer::RECONNECT_DEBOUNCE_SECS giây thì bỏ qua (tránh conflict worker + liveness).
    async fn reconnect(&self) -> Result<(), String> {
        {
            let mut last = self.last_reconnect.lock();
            if let Some(t) = *last {
                if t.elapsed()
                    < std::time::Duration::from_secs(kafka_producer::RECONNECT_DEBOUNCE_SECS)
                {
                    tracing::debug!("[Kafka] reconnect skipped (debounce)");
                    return Ok(());
                }
            }
        }
        let new_client = timeout(
            Duration::from_secs(kafka_producer::RECONNECT_TIMEOUT_SECS),
            Self::build_client(&self.config),
        )
        .await
        .map_err(|_| {
            format!(
                "reconnect timeout after {}s",
                kafka_producer::RECONNECT_TIMEOUT_SECS
            )
        })?
        .map_err(|e| format!("reconnect: {}", e))?;
        let mut guard = self.client.write().await;
        *guard = new_client;
        drop(guard);
        *self.partition_count_cache.write() = None;
        *self.last_reconnect.lock() = Some(Instant::now());
        tracing::info!(brokers = %self.config.bootstrap_servers, "[Kafka] reconnected");
        Ok(())
    }

    /// Gửi bản tin checkin (có retry + backoff trong task nền).
    pub async fn send_checkin(&self, event: CheckinEvent) {
        let payload = match serde_json::to_vec(&event) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(event_id = %event.event_id, error = %e, "[Kafka] checkin serialize error");
                return;
            }
        };
        let event_id = event.event_id.clone();
        if let Err(e) = self
            .send_raw_with_retry(
                self.topic_trans_hub_online.as_ref(),
                payload,
                Some(event_id.clone()),
            )
            .await
        {
            tracing::error!(event_id = %event_id, error = %e, "[Kafka] checkin send failed after retries");
        }
    }

    /// Gửi bản tin checkout (có retry + backoff trong task nền).
    pub async fn send_checkout(&self, event: CheckoutEvent) {
        let payload = match serde_json::to_vec(&event) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(event_id = %event.event_id, error = %e, "[Kafka] checkout serialize error");
                return;
            }
        };
        let event_id = event.event_id.clone();
        if let Err(e) = self
            .send_raw_with_retry(
                self.topic_trans_hub_online.as_ref(),
                payload,
                Some(event_id.clone()),
            )
            .await
        {
            tracing::error!(event_id = %event_id, error = %e, "[Kafka] checkout send failed after retries");
        }
    }

    /// Gửi với retry + exponential backoff. Sau 3 lần timeout liên tiếp thì reconnect client rồi retry.
    async fn send_raw_with_retry(
        &self,
        topic: &str,
        payload: Vec<u8>,
        key: Option<String>,
    ) -> Result<(), String> {
        let max_retries = self.config.retries.max(1);
        let initial_ms = self.config.retry_initial_delay_ms;
        let max_ms = self.config.retry_max_delay_ms;
        let mut consecutive_timeouts: u32 = 0;

        for attempt in 0..max_retries {
            let last_attempt = attempt == max_retries - 1;
            match self.send_raw(topic, payload.clone(), key.clone()).await {
                Ok(()) => {
                    tracing::debug!(topic, attempt, key = ?key, "[Kafka] send ok");
                    return Ok(());
                }
                Err(e) => {
                    let is_timeout = e.contains("timeout") || e.contains("connection hung");
                    if is_timeout {
                        consecutive_timeouts += 1;
                        if consecutive_timeouts
                            >= kafka_producer::RECONNECT_AFTER_CONSECUTIVE_TIMEOUTS
                        {
                            consecutive_timeouts = 0;
                            self.request_reconnect();
                        }
                    } else {
                        consecutive_timeouts = 0;
                    }
                    if last_attempt {
                        return Err(e);
                    }
                    let delay_ms = (initial_ms * (1 << attempt)).min(max_ms);
                    tracing::warn!(
                        attempt = attempt + 1,
                        max_retries,
                        topic,
                        delay_ms,
                        error = %e,
                        "[Kafka] send failed, retrying"
                    );
                    sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }
            }
        }
        unreachable!("retry loop always returns on last_attempt")
    }

    /// Chọn partition từ key (cùng key → cùng partition). Không key thì partition 0.
    /// Tương thích logic partition mặc định: hash(key) % num_partitions.
    fn partition_for_key(key: Option<&str>, num_partitions: i32) -> i32 {
        if num_partitions <= 1 {
            return 0;
        }
        let n = num_partitions as u32;
        let h = match key {
            Some(k) => k
                .bytes()
                .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)),
            None => 0,
        };
        (h % n) as i32
    }

    /// Partition cuối cùng: ưu tiên config fixed_partition (nếu set), ngược lại chọn theo key. Luôn clamp trong [0, num_partitions-1].
    fn resolve_partition_id(&self, key: Option<&str>, num_partitions: i32) -> i32 {
        let raw = match self.config.topic_trans_fixed_partition {
            Some(p) => p,
            None => Self::partition_for_key(key, num_partitions),
        };
        raw.clamp(0, (num_partitions - 1).max(0))
    }

    /// Lấy số partition của topic (cache; refetch khi cache None hoặc sau reconnect).
    /// Ưu tiên config topic_trans_partition_count nếu set (không gọi list_topics → hạn chế rủi ro timeout/cluster lớn).
    /// Nếu gọi list_topics: bọc timeout; lỗi hoặc timeout → fallback partition 1 và cache để hệ thống vẫn gửi được.
    /// Số partition từ metadata được cap bởi kafka_producer::MAX_PARTITIONS_FROM_METADATA để tránh batch gửi quá nhiều round-trip.
    async fn get_partition_count_cached(&self, topic: &str) -> Result<i32, String> {
        {
            let cache = self.partition_count_cache.read();
            if let Some(n) = *cache {
                return Ok(n);
            }
        }
        if topic == self.topic_trans_hub_online.as_ref() {
            if let Some(n) = self.config.topic_trans_partition_count {
                let n = n.max(1);
                *self.partition_count_cache.write() = Some(n);
                tracing::debug!(topic = %topic, partition_count = n, "[Kafka] partition count from config (no list_topics)");
                return Ok(n);
            }
        }
        let timeout_dur = Duration::from_millis(self.config.request_timeout_ms as u64);
        let client_lock = Arc::clone(&self.client);
        let topic_owned = topic.to_string();
        let list_result = timeout(timeout_dur, async move {
            let guard = client_lock.read().await;
            guard.list_topics().await
        })
        .await;
        let n = match list_result {
            Ok(Ok(topics)) => {
                let raw = topics
                    .into_iter()
                    .find(|t| t.name == topic_owned)
                    .map(|t| t.partitions.len() as i32)
                    .unwrap_or(1)
                    .max(1);
                raw.min(kafka_producer::MAX_PARTITIONS_FROM_METADATA)
            }
            Ok(Err(e)) => {
                tracing::warn!(
                    topic = %topic,
                    error = %e,
                    "[Kafka] list_topics failed, fallback to 1 partition (messages will use partition 0)"
                );
                1
            }
            Err(_) => {
                tracing::warn!(
                    topic = %topic,
                    timeout_ms = self.config.request_timeout_ms,
                    "[Kafka] list_topics timeout, fallback to 1 partition (messages will use partition 0)"
                );
                1
            }
        };
        if n < 1 {
            *self.partition_count_cache.write() = Some(1);
            Ok(1)
        } else {
            *self.partition_count_cache.write() = Some(n);
            tracing::debug!(topic = %topic, partition_count = n, "[Kafka] partition count cached from metadata");
            Ok(n)
        }
    }

    /// Gửi một lần. Trả về lỗi để caller retry.
    /// Partition: nếu config topic_trans_fixed_partition set thì dùng partition đó; ngược lại chọn theo key (hash % num_partitions).
    /// Bọc toàn bộ I/O bằng timeout để tránh kết nối treo vô hạn (hung connection).
    async fn send_raw(
        &self,
        topic: &str,
        payload: Vec<u8>,
        key: Option<String>,
    ) -> Result<(), String> {
        let num_partitions = self.get_partition_count_cached(topic).await?;
        let partition_id = self.resolve_partition_id(key.as_deref(), num_partitions);
        let timeout_dur = Duration::from_millis(self.config.request_timeout_ms as u64);
        let client_lock = Arc::clone(&self.client);
        let topic_owned = topic.to_string();
        let key_bytes = key.as_deref().map(|k| k.as_bytes().to_vec());
        let record = Record {
            key: key_bytes,
            value: Some(payload),
            headers: EMPTY_HEADERS.clone(),
            timestamp: Utc::now(),
        };
        let result = timeout(timeout_dur, async move {
            let guard = client_lock.read().await;
            let partition_client = guard
                .partition_client(topic_owned, partition_id, UnknownTopicHandling::Retry)
                .await
                .map_err(|e| format!("partition_client: {}", e))?;
            partition_client
                .produce(
                    vec![record],
                    rskafka::client::partition::Compression::default(),
                )
                .await
                .map_err(|e| format!("produce: {}", e))?;
            Ok::<(), String>(())
        })
        .await
        .map_err(|_| {
            format!(
                "request timeout after {}ms (connection hung?)",
                self.config.request_timeout_ms
            )
        })?;
        result?;
        Ok(())
    }

    /// Gửi một batch record lên Kafka; mỗi record gửi vào partition theo key (hash(key) % num_partitions).
    /// Records được nhóm theo partition rồi produce từng nhóm (nhiều round-trip nếu nhiều partition).
    /// Bọc toàn bộ I/O bằng timeout để tránh kết nối treo (hung connection).
    /// Timeout tăng theo số record và số partition (base + 30ms/record, tối đa 2x base).
    async fn send_raw_batch(
        &self,
        topic: &str,
        records: &[(Option<String>, Vec<u8>)],
    ) -> Result<(), String> {
        if records.is_empty() {
            return Ok(());
        }
        let num_partitions = self.get_partition_count_cached(topic).await?;
        let now = Utc::now();
        let mut by_partition: HashMap<i32, Vec<Record>> = HashMap::new();
        for (key, payload) in records {
            let partition_id = self.resolve_partition_id(key.as_deref(), num_partitions);
            let record = Record {
                key: key.as_deref().map(|k| k.as_bytes().to_vec()),
                value: Some(payload.clone()),
                headers: EMPTY_HEADERS.clone(),
                timestamp: now,
            };
            by_partition.entry(partition_id).or_default().push(record);
        }
        let num_partitions_in_batch = by_partition.len();
        let base_ms = self.config.request_timeout_ms as u64;
        let extra_ms = (records.len() as u64).saturating_mul(30).min(base_ms);
        // Nhiều partition → nhiều round-trip produce(); tăng timeout để tránh timeout khi topic có nhiều partition.
        let partition_overhead_ms = (num_partitions_in_batch as u64).saturating_mul(50);
        let timeout_ms = base_ms
            .saturating_add(extra_ms)
            .saturating_add(partition_overhead_ms);
        let timeout_dur = Duration::from_millis(timeout_ms);
        let client_lock = Arc::clone(&self.client);
        let topic_owned = topic.to_string();
        let result = timeout(timeout_dur, async move {
            let guard = client_lock.read().await;
            for (partition_id, kafka_records) in by_partition {
                let partition_client = guard
                    .partition_client(
                        topic_owned.clone(),
                        partition_id,
                        UnknownTopicHandling::Retry,
                    )
                    .await
                    .map_err(|e| format!("partition_client({}): {}", partition_id, e))?;
                partition_client
                    .produce(
                        kafka_records,
                        rskafka::client::partition::Compression::default(),
                    )
                    .await
                    .map_err(|e| format!("produce(partition {}): {}", partition_id, e))?;
            }
            Ok::<(), String>(())
        })
        .await
        .map_err(|_| {
            format!(
                "request timeout after {}ms (connection hung?)",
                self.config.request_timeout_ms
            )
        })?;
        result?;
        Ok(())
    }

    /// Gửi batch với retry + backoff. Sau 3 lần timeout liên tiếp thì reconnect client rồi retry.
    async fn send_raw_batch_with_retry(
        &self,
        topic: &str,
        records: &[(Option<String>, Vec<u8>)],
    ) -> Result<(), String> {
        if records.is_empty() {
            return Ok(());
        }
        let max_retries = self.config.retries.max(1);
        let initial_ms = self.config.retry_initial_delay_ms;
        let max_ms = self.config.retry_max_delay_ms;
        let mut consecutive_timeouts: u32 = 0;

        for attempt in 0..max_retries {
            let last_attempt = attempt == max_retries - 1;
            match self.send_raw_batch(topic, records).await {
                Ok(()) => {
                    tracing::debug!(
                        topic,
                        records = records.len(),
                        attempt,
                        "[Kafka] batch send ok"
                    );
                    return Ok(());
                }
                Err(e) => {
                    let is_timeout = e.contains("timeout") || e.contains("connection hung");
                    if is_timeout {
                        consecutive_timeouts += 1;
                        if consecutive_timeouts
                            >= kafka_producer::RECONNECT_AFTER_CONSECUTIVE_TIMEOUTS
                        {
                            consecutive_timeouts = 0;
                            self.request_reconnect();
                        }
                    } else {
                        consecutive_timeouts = 0;
                    }
                    if last_attempt {
                        return Err(e);
                    }
                    let delay_ms = (initial_ms * (1 << attempt)).min(max_ms);
                    tracing::warn!(
                        attempt = attempt + 1,
                        max_retries,
                        topic,
                        delay_ms,
                        error = %e,
                        "[Kafka] batch send failed, retrying"
                    );
                    sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }
            }
        }
        unreachable!("retry loop always returns on last_attempt")
    }

    /// Build (event_id, json_bytes) cho checkin. Trả None nếu serialize lỗi.
    fn build_checkin_record(&self, p: &CheckinCommitPayload) -> Option<(String, Vec<u8>)> {
        let now_ms = timestamp_ms();
        let event_id = uuid::Uuid::new_v4().to_string();
        let event = CheckinEvent {
            event_id: event_id.clone(),
            event_type: kafka_producer::EVENT_TYPE_CHECKIN.to_string(),
            timestamp: now_ms,
            trace_id: event_id.clone(),
            data_version: self.data_version.to_string(),
            data: CheckinEventData {
                request_id: p.request_id,
                tid: p.tid.clone(),
                ticket_in_id: p.ticket_in_id,
                ticket_eTag_id: p.ticket_eTag_id,
                etag: p.etag.clone(),
                plate: p.plate.clone(),
                station_in: p.station_in,
                lane_in: p.lane_in,
                checkin_datetime: p.checkin_datetime,
                checkin_commit_datetime: p.checkin_commit_datetime,
                status: p.status,
                register_vehicle_type: p.register_vehicle_type.clone(),
                seat: p.seat,
                weight_goods: p.weight_goods,
                weight_all: p.weight_all,
                vehicle_type: p.vehicle_type,
            },
        };
        serde_json::to_vec(&event).ok().map(|v| (event_id, v))
    }

    /// Build (event_id, json_bytes) cho checkout. Trả None nếu serialize lỗi.
    fn build_checkout_record(&self, p: &CheckoutCommitPayload) -> Option<(String, Vec<u8>)> {
        let request_id = p.request_id;
        let etag = p.etag.clone();
        let rating_detail_count = p.rating_detail.len();

        let now_ms = timestamp_ms();
        let event_id = uuid::Uuid::new_v4().to_string();
        let rating_detail_line = rating_detail_count as i32;
        let event = CheckoutEvent {
            event_id: event_id.clone(),
            event_type: kafka_producer::EVENT_TYPE_CHECKOUT.to_string(),
            timestamp: now_ms,
            trace_id: event_id.clone(),
            data_version: self.data_version.to_string(),
            data: CheckoutEventData {
                request_id: p.request_id,
                tid: p.tid.clone(),
                ticket_in_id: p.ticket_in_id,
                ticket_eTag_id: p.ticket_eTag_id,
                hub_id: p.hub_id,
                ticket_out_id: p.ticket_out_id,
                etag: p.etag.clone(),
                plate: p.plate.clone(),
                station_in: p.station_in,
                lane_in: p.lane_in,
                checkin_datetime: p.checkin_datetime,
                checkin_commit_datetime: p.checkin_commit_datetime,
                station_out: p.station_out,
                lane_out: p.lane_out,
                checkout_datetime: p.checkout_datetime,
                checkout_commit_datetime: p.checkout_commit_datetime,
                register_vehicle_type: p.register_vehicle_type.clone(),
                seat: p.seat,
                weight_goods: p.weight_goods,
                weight_all: p.weight_all,
                vehicle_type: p.vehicle_type,
                ticket_type: p.ticket_type.clone(),
                price_ticket_type: p.price_ticket_type,
                status: p.status,
                trans_amount: p.trans_amount,
                rating_detail_line,
                rating_detail: p.rating_detail.clone(),
            },
        };
        tracing::debug!(request_id, etag = %etag, rating_detail_count, "[Kafka] building checkout record");
        if rating_detail_count == 0 {
            tracing::debug!(request_id, etag = %etag, "[Kafka] checkout rating_detail empty");
        }
        match serde_json::to_vec(&event) {
            Ok(bytes) => {
                if let Ok(json_str) = std::str::from_utf8(&bytes) {
                    if !json_str.contains("rating_detail") {
                        tracing::warn!(request_id, etag = %etag, rating_detail_count, "[Kafka] rating_detail missing in JSON");
                    } else if json_str.contains("\"rating_detail\":[]") {
                        tracing::debug!(request_id, etag = %etag, "[Kafka] rating_detail empty array in JSON");
                    }
                }
                Some((event_id, bytes))
            }
            Err(e) => {
                tracing::error!(request_id, etag = %etag, rating_detail_count, error = %e, "[Kafka] checkout serialize failed");
                None
            }
        }
    }

    /// Tạo và gửi CHECKIN_INFO từ dữ liệu commit làn vào.
    pub async fn send_checkin_from_commit(
        &self,
        request_id: i64,
        tid: &str,
        ticket_in_id: i64,
        ticket_eTag_id: i64,
        etag: &str,
        plate: &str,
        station_in: i32,
        lane_in: i32,
        checkin_datetime: i64,
        checkin_commit_datetime: i64,
        status: i32,
        register_vehicle_type: &str,
        seat: i32,
        weight_goods: i32,
        weight_all: i32,
        vehicle_type: i32,
    ) {
        let now_ms = timestamp_ms();
        let event_id = uuid::Uuid::new_v4().to_string();
        let event = CheckinEvent {
            event_id: event_id.clone(),
            event_type: kafka_producer::EVENT_TYPE_CHECKIN.to_string(),
            timestamp: now_ms,
            trace_id: event_id,
            data_version: self.data_version.to_string(),
            data: CheckinEventData {
                request_id,
                tid: tid.to_string(),
                ticket_in_id,
                ticket_eTag_id,
                etag: etag.to_string(),
                plate: plate.to_string(),
                station_in,
                lane_in,
                checkin_datetime,
                checkin_commit_datetime,
                status,
                register_vehicle_type: register_vehicle_type.to_string(),
                seat,
                weight_goods,
                weight_all,
                vehicle_type,
            },
        };
        self.send_checkin(event).await;
    }

    /// Tạo và gửi CHECKOUT_INFO từ dữ liệu commit làn ra.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_checkout_from_commit(
        &self,
        request_id: i64,
        tid: &str,
        ticket_in_id: i64,
        ticket_eTag_id: i64,
        hub_id: Option<i64>,
        ticket_out_id: i64,
        etag: &str,
        plate: &str,
        station_in: i32,
        lane_in: i32,
        checkin_datetime: i64,
        checkin_commit_datetime: i64,
        station_out: i32,
        lane_out: i32,
        checkout_datetime: i64,
        checkout_commit_datetime: i64,
        register_vehicle_type: &str,
        seat: i32,
        weight_goods: i32,
        weight_all: i32,
        vehicle_type: i32,
        ticket_type: &str,
        price_ticket_type: i32,
        status: i32,
        trans_amount: i32,
        rating_detail: Vec<BOORatingDetail>,
    ) {
        let now_ms = timestamp_ms();
        let event_id = uuid::Uuid::new_v4().to_string();
        let rating_detail_line = rating_detail.len() as i32;
        let event = CheckoutEvent {
            event_id: event_id.clone(),
            event_type: kafka_producer::EVENT_TYPE_CHECKOUT.to_string(),
            timestamp: now_ms,
            trace_id: event_id,
            data_version: self.data_version.to_string(),
            data: CheckoutEventData {
                request_id,
                tid: tid.to_string(),
                ticket_in_id,
                ticket_eTag_id,
                hub_id,
                ticket_out_id,
                etag: etag.to_string(),
                plate: plate.to_string(),
                station_in,
                lane_in,
                checkin_datetime,
                checkin_commit_datetime,
                station_out,
                lane_out,
                checkout_datetime,
                checkout_commit_datetime,
                register_vehicle_type: register_vehicle_type.to_string(),
                seat,
                weight_goods,
                weight_all,
                vehicle_type,
                ticket_type: ticket_type.to_string(),
                price_ticket_type,
                status,
                trans_amount,
                rating_detail_line,
                rating_detail,
            },
        };
        self.send_checkout(event).await;
    }

    /// Đẩy event checkin vào hàng đợi (không block). Khi đầy thì bỏ event cũ nhất, luôn giữ event mới.
    #[inline]
    pub fn send_checkin_from_commit_background(self: Arc<Self>, payload: CheckinCommitPayload) {
        tracing::info!(
            request_id = payload.request_id,
            etag = %payload.etag,
            "[Kafka] event queued checkin"
        );
        self.queue.push(KafkaMessage::Checkin(payload));
    }

    /// Đẩy event checkout vào hàng đợi (không block). Khi đầy thì bỏ event cũ nhất, luôn giữ event mới.
    #[inline]
    pub fn send_checkout_from_commit_background(self: Arc<Self>, payload: CheckoutCommitPayload) {
        tracing::info!(
            request_id = payload.request_id,
            etag = %payload.etag,
            "[Kafka] event queued checkout"
        );
        self.queue.push(KafkaMessage::Checkout(payload));
    }

    /// Đẩy nhiều message vào queue (dùng khi drain pending queue sau khi producer ready).
    pub fn push_messages(&self, messages: Vec<KafkaMessage>) {
        for msg in messages {
            self.queue.push(msg);
        }
    }
}
