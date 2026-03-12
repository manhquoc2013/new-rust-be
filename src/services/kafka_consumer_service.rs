//! Kafka consumer: nhận bản tin đồng bộ checkin từ HUB (topic topics.hub.checkin.trans.online).
//! - `run_checkin_hub_consumer`: vòng lặp fetch → parse JSON (CHECKIN_HUB_INFO) → cập nhật hub_id trong TRANSPORT_TRANSACTION_STAGE (BECT).
//! - `load_offset_from_file` / `save_offset_to_file`: đọc/ghi offset để resume. `build_consumer_client`: tạo client (SASL nếu có).
//! Luồng: build client → load offset → fetch batch → xử lý song song từng record (spawn) → commit offset khi cả batch xong.
//! HA: khi truyền KeyDB (keydb_opt), chỉ pod nào giữ leader lock trong KeyDB mới chạy consumer; các pod khác chờ và retry.

use crate::cache::config::keydb::KeyDB;
use crate::configs::kafka::KafkaConfig;
use crate::constants::kafka_consumer;
use crate::db::repositories::TransportTransactionStageRepository;
use crate::db::{DbError, Repository};
use crate::models::hub_events::{
    CheckinHubInfoEvent, CheckinHubInfoEventData, CheckinHubInfoEventMinimal,
};
use crate::models::ETDR::get_etdr_cache;
use rskafka::client::partition::{OffsetAt, UnknownTopicHandling};
use rskafka::client::{ClientBuilder, Credentials, SaslConfig};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing;

/// Đọc offset đã lưu từ file (mỗi dòng một số). Trả về None nếu file không tồn tại hoặc lỗi.
fn load_offset_from_file(path: &Path) -> Option<i64> {
    let s = fs::read_to_string(path).ok()?;
    let s = s.trim();
    s.parse::<i64>().ok()
}

/// Ghi offset ra file (ghi đè).
fn save_offset_to_file(path: &Path, offset: i64) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{}\n", offset))
}

/// Lấy danh sách partition hợp lệ của topic từ metadata (list_topics), không probe từng partition.
/// Chỉ consume từ các partition thực sự tồn tại trên broker, tránh gọi partition không có → không log ERROR.
async fn get_valid_partitions(
    client: Arc<RwLock<rskafka::client::Client>>,
    topic: &str,
) -> Result<Vec<i32>, String> {
    let guard = client.read().await;
    let topics = guard
        .list_topics()
        .await
        .map_err(|e| format!("list_topics: {}", e))?;
    drop(guard);
    let t = topics.into_iter().find(|t| t.name == topic);
    match t {
        Some(t) => {
            let partitions: Vec<i32> = t.partitions.into_iter().collect();
            tracing::info!(
                topic = %topic,
                partition_count = partitions.len(),
                partitions = ?partitions,
                "[Kafka] valid partitions from metadata"
            );
            Ok(partitions)
        }
        None => Err(format!("Topic {} not found in cluster metadata", topic)),
    }
}

/// Build Kafka client (giống producer: SASL nếu có username/password).
async fn build_consumer_client(
    config: &KafkaConfig,
) -> Result<rskafka::client::Client, Box<dyn std::error::Error + Send + Sync>> {
    let brokers: Vec<String> = config
        .bootstrap_servers
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let mut builder = ClientBuilder::new(brokers.clone());

    if let (Some(username), Some(password)) = (&config.username, &config.password) {
        let credentials = Credentials {
            username: username.clone(),
            password: password.clone(),
        };
        builder = builder.sasl_config(SaslConfig::Plain(credentials));
        tracing::info!(username = %username, "[Kafka] consumer SASL enabled");
    }

    builder
        .build()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}

/// Reconnect Kafka client khi connection bị ngắt
async fn reconnect_consumer_client(
    client: Arc<RwLock<rskafka::client::Client>>,
    config: &KafkaConfig,
) -> Result<(), String> {
    let new_client = build_consumer_client(config)
        .await
        .map_err(|e| format!("reconnect failed: {}", e))?;
    let mut guard = client.write().await;
    *guard = new_client;
    drop(guard);
    tracing::info!(brokers = %config.bootstrap_servers, "[Kafka] consumer reconnected");
    Ok(())
}

/// Consume từ một partition cụ thể
async fn consume_partition(
    client: Arc<RwLock<rskafka::client::Client>>,
    topic: String,
    partition_id: i32,
    offset_file_path: std::path::PathBuf,
    config: KafkaConfig,
) {
    let repo_transport = Arc::new(TransportTransactionStageRepository::new());

    // File offset riêng cho mỗi partition
    let offset_file = offset_file_path.join(format!("partition_{}", partition_id));
    let mut next_offset = load_offset_from_file(offset_file.as_path()).unwrap_or_else(|| {
        tracing::debug!(
            partition_id,
            "[Kafka] consumer partition no offset file, will use get_offset(Latest)"
        );
        -1
    });

    // Nếu chưa có offset, lấy từ Latest
    if next_offset < 0 {
        let mut retry_count = 0;
        const MAX_INIT_RETRIES: u32 = 3;

        loop {
            let guard = client.read().await;
            match guard
                .partition_client(topic.clone(), partition_id, UnknownTopicHandling::Retry)
                .await
            {
                Ok(p) => match p.get_offset(OffsetAt::Latest).await {
                    Ok(off) => {
                        next_offset = off;
                        tracing::debug!(topic = %topic, partition_id, next_offset, "[Kafka] consumer partition starting from offset");
                        break;
                    }
                    Err(e) => {
                        tracing::error!(partition_id, error = %e, "[Kafka] consumer get_offset(Latest) failed");
                        if retry_count >= MAX_INIT_RETRIES {
                            tracing::error!(
                                partition_id,
                                "[Kafka] consumer failed to initialize after retries"
                            );
                            return;
                        }
                        retry_count += 1;
                        drop(guard);
                        tokio::time::sleep(Duration::from_millis(1000)).await;
                        continue;
                    }
                },
                Err(e) => {
                    // Partition có thể đã bị xóa hoặc connection issue
                    tracing::warn!(topic = %topic, partition_id, error = %e, retry_count, "[Kafka] consumer partition_client failed");

                    if retry_count >= MAX_INIT_RETRIES {
                        // Thử reconnect một lần trước khi give up
                        drop(guard);
                        if let Err(reconnect_err) =
                            reconnect_consumer_client(Arc::clone(&client), &config).await
                        {
                            tracing::error!(partition_id, error = %reconnect_err, "[Kafka] consumer reconnect failed");
                        }

                        // Thử lại sau khi reconnect
                        let guard2 = client.read().await;
                        match guard2
                            .partition_client(
                                topic.clone(),
                                partition_id,
                                UnknownTopicHandling::Retry,
                            )
                            .await
                        {
                            Ok(p2) => {
                                match p2.get_offset(OffsetAt::Latest).await {
                                    Ok(off) => {
                                        next_offset = off;
                                        tracing::info!(topic = %topic, partition_id, next_offset, "[Kafka] consumer partition recovered after reconnect");
                                        break;
                                    }
                                    Err(e2) => {
                                        tracing::error!(partition_id, error = %e2, "[Kafka] consumer partition may be deleted, stopping consumer");
                                        return; // Partition đã bị xóa, dừng consumer cho partition này
                                    }
                                }
                            }
                            Err(e2) => {
                                tracing::error!(topic = %topic, partition_id, error = %e2, "[Kafka] consumer partition does not exist (deleted?), stopping consumer");
                                return; // Partition đã bị xóa, dừng consumer cho partition này
                            }
                        }
                    } else {
                        retry_count += 1;
                        drop(guard);
                        tokio::time::sleep(Duration::from_millis(1000)).await;
                        continue;
                    }
                }
            }
        }
    }

    let request_timeout = Duration::from_millis(config.request_timeout_ms as u64);
    let idle_sleep = Duration::from_millis(kafka_consumer::POLL_IDLE_SLEEP_MS);
    let mut last_heartbeat = std::time::Instant::now();
    const HEARTBEAT_INTERVAL_SECS: u64 = 60; // Log heartbeat mỗi 60 giây
    const RECONNECT_AFTER_CONSECUTIVE_ERRORS: u32 = 5; // Reconnect sau 5 lỗi liên tiếp
    let mut consecutive_errors: u32 = 0;
    let mut last_reconnect_attempt = std::time::Instant::now();
    const RECONNECT_DEBOUNCE_SECS: u64 = 10; // Debounce reconnect

    loop {
        let partition_client = {
            let guard = client.read().await;
            match guard
                .partition_client(topic.clone(), partition_id, UnknownTopicHandling::Retry)
                .await
            {
                Ok(p) => {
                    consecutive_errors = 0; // Reset counter khi thành công
                    p
                }
                Err(e) => {
                    consecutive_errors += 1;
                    tracing::warn!(partition_id, error = %e, consecutive_errors, "[Kafka] consumer partition_client failed");

                    // Thử reconnect nếu có nhiều lỗi liên tiếp và đã qua debounce period
                    if consecutive_errors >= RECONNECT_AFTER_CONSECUTIVE_ERRORS {
                        let now = std::time::Instant::now();
                        if now.duration_since(last_reconnect_attempt).as_secs()
                            >= RECONNECT_DEBOUNCE_SECS
                        {
                            last_reconnect_attempt = now;
                            consecutive_errors = 0; // Reset để tránh reconnect liên tục
                            if let Err(reconnect_err) =
                                reconnect_consumer_client(Arc::clone(&client), &config).await
                            {
                                tracing::warn!(partition_id, error = %reconnect_err, "[Kafka] consumer reconnect failed");
                            }
                        }
                    }

                    tokio::time::sleep(idle_sleep).await;
                    continue;
                }
            }
        };

        let fetch_result = timeout(
            request_timeout,
            partition_client.fetch_records(
                next_offset,
                kafka_consumer::FETCH_MIN_BYTES..kafka_consumer::FETCH_MAX_BYTES,
                kafka_consumer::FETCH_MAX_WAIT_MS,
            ),
        )
        .await;

        let (records, high_watermark) = match fetch_result {
            Ok(Ok((r, h))) => {
                consecutive_errors = 0; // Reset counter khi thành công
                (r, h)
            }
            Ok(Err(e)) => {
                consecutive_errors += 1;
                tracing::warn!(partition_id, error = %e, consecutive_errors, "[Kafka] consumer fetch_records error");

                // Thử reconnect nếu có nhiều lỗi liên tiếp
                if consecutive_errors >= RECONNECT_AFTER_CONSECUTIVE_ERRORS {
                    let now = std::time::Instant::now();
                    if now.duration_since(last_reconnect_attempt).as_secs()
                        >= RECONNECT_DEBOUNCE_SECS
                    {
                        last_reconnect_attempt = now;
                        consecutive_errors = 0;
                        if let Err(reconnect_err) =
                            reconnect_consumer_client(Arc::clone(&client), &config).await
                        {
                            tracing::warn!(partition_id, error = %reconnect_err, "[Kafka] consumer reconnect failed");
                        }
                    }
                }

                tokio::time::sleep(idle_sleep).await;
                continue;
            }
            Err(_) => {
                consecutive_errors += 1;
                tracing::debug!(
                    partition_id,
                    consecutive_errors,
                    "[Kafka] consumer fetch timeout"
                );

                // Thử reconnect nếu có nhiều timeout liên tiếp
                if consecutive_errors >= RECONNECT_AFTER_CONSECUTIVE_ERRORS {
                    let now = std::time::Instant::now();
                    if now.duration_since(last_reconnect_attempt).as_secs()
                        >= RECONNECT_DEBOUNCE_SECS
                    {
                        last_reconnect_attempt = now;
                        consecutive_errors = 0;
                        if let Err(reconnect_err) =
                            reconnect_consumer_client(Arc::clone(&client), &config).await
                        {
                            tracing::warn!(partition_id, error = %reconnect_err, "[Kafka] consumer reconnect failed");
                        }
                    }
                }

                tokio::time::sleep(idle_sleep).await;
                continue;
            }
        };

        if records.is_empty() {
            // Log heartbeat định kỳ để biết consumer vẫn đang hoạt động
            let now = std::time::Instant::now();
            if now.duration_since(last_heartbeat).as_secs() >= HEARTBEAT_INTERVAL_SECS {
                tracing::debug!(
                    partition_id,
                    next_offset,
                    high_watermark,
                    "[Kafka] consumer heartbeat (no new messages, waiting...)"
                );
                last_heartbeat = now;
            } else {
                tracing::debug!(
                    partition_id,
                    next_offset,
                    high_watermark,
                    "[Kafka] consumer fetch empty (no new messages, waiting...)"
                );
            }
            tokio::time::sleep(idle_sleep).await;
            continue;
        }

        // Reset heartbeat timer khi có message
        last_heartbeat = std::time::Instant::now();

        tracing::info!(
            partition_id,
            record_count = records.len(),
            next_offset,
            high_watermark,
            "[Kafka] consumer fetched records"
        );

        let mut last_offset = next_offset;
        let mut handles = Vec::new();
        for record_and_offset in &records {
            last_offset = record_and_offset.offset;
            if let Some(ref value) = record_and_offset.record.value {
                // Thử parse với format có envelope (CheckinHubInfoEvent)
                match serde_json::from_slice::<CheckinHubInfoEvent>(value) {
                    Ok(event) => {
                        if event.event_type == kafka_consumer::EVENT_TYPE_CHECKIN_HUB_INFO {
                            let Some(hub_id) = event.data.hub_id else {
                                tracing::error!(
                                    event_id = %event.event_id,
                                    "[Kafka] consumer CHECKIN_HUB_INFO missing hub_id, skip record"
                                );
                                continue;
                            };
                            let Some(ticket_in_id) = event.data.ticket_in_id else {
                                tracing::error!(
                                    event_id = %event.event_id,
                                    hub_id,
                                    "[Kafka] consumer CHECKIN_HUB_INFO missing ticket_in_id, skip record"
                                );
                                continue;
                            };
                            let event_id = event.event_id.clone();
                            let repo_transport = Arc::clone(&repo_transport);
                            let handle = tokio::spawn(async move {
                                let r_transport = tokio::task::spawn_blocking(move || {
                                    repo_transport
                                        .update_hub_id_by_transport_trans_id(ticket_in_id, hub_id)
                                })
                                .await
                                .unwrap_or_else(|e| Err(DbError::ExecutionError(e.to_string())));
                                let ok_transport = r_transport.is_ok();
                                let updated_etdr = get_etdr_cache()
                                    .update_hub_id_by_ticket_id(ticket_in_id, hub_id);
                                if ok_transport || updated_etdr {
                                    tracing::info!(
                                        ticket_in_id,
                                        hub_id,
                                        event_id = %event_id,
                                        updated_transport = ok_transport,
                                        updated_etdr = updated_etdr,
                                        "[Kafka] consumer updated HUB_ID by ticket_in_id (TRANSPORT_TRANS_ID/ticket_id)"
                                    );
                                }
                                if let Err(e) = r_transport {
                                    tracing::warn!(ticket_in_id, hub_id, error = %e, "[Kafka] consumer TRANSPORT_TRANSACTION_STAGE update failed");
                                }
                            });
                            handles.push(handle);
                        } else {
                            tracing::debug!(
                                event_type = %event.event_type,
                                event_id = %event.event_id,
                                "[Kafka] consumer skip non CHECKIN_HUB_INFO"
                            );
                        }
                    }
                    Err(_) => {
                        // Thử parse envelope tối giản (chỉ timestamp + data, không có event_id/event_type/trace_id/data_version)
                        match serde_json::from_slice::<CheckinHubInfoEventMinimal>(value) {
                            Ok(minimal) => {
                                let Some(hub_id) = minimal.data.hub_id else {
                                    tracing::error!(
                                        "[Kafka] consumer CHECKIN_HUB_INFO (minimal) missing hub_id, skip record"
                                    );
                                    continue;
                                };
                                let Some(ticket_in_id) = minimal.data.ticket_in_id else {
                                    tracing::error!(
                                        hub_id,
                                        "[Kafka] consumer CHECKIN_HUB_INFO (minimal) missing ticket_in_id, skip record"
                                    );
                                    continue;
                                };
                                let repo_transport = Arc::clone(&repo_transport);
                                let handle = tokio::spawn(async move {
                                    let r_transport = tokio::task::spawn_blocking(move || {
                                        repo_transport.update_hub_id_by_transport_trans_id(
                                            ticket_in_id,
                                            hub_id,
                                        )
                                    })
                                    .await
                                    .unwrap_or_else(|e| {
                                        Err(DbError::ExecutionError(e.to_string()))
                                    });
                                    let ok_transport = r_transport.is_ok();
                                    let updated_etdr = get_etdr_cache()
                                        .update_hub_id_by_ticket_id(ticket_in_id, hub_id);
                                    if ok_transport || updated_etdr {
                                        tracing::info!(
                                            ticket_in_id,
                                            hub_id,
                                            updated_transport = ok_transport,
                                            updated_etdr = updated_etdr,
                                            "[Kafka] consumer updated HUB_ID by ticket_in_id (minimal envelope timestamp+data)"
                                        );
                                    }
                                    if let Err(e) = r_transport {
                                        tracing::warn!(ticket_in_id, hub_id, error = %e, "[Kafka] consumer TRANSPORT_TRANSACTION_STAGE update failed");
                                    }
                                });
                                handles.push(handle);
                            }
                            Err(_) => {
                                // Thử parse với format không có envelope (chỉ data payload)
                                match serde_json::from_slice::<CheckinHubInfoEventData>(value) {
                                    Ok(data) => {
                                        let Some(hub_id) = data.hub_id else {
                                            tracing::error!(
                                                "[Kafka] consumer CHECKIN_HUB_INFO (direct payload) missing hub_id, skip record"
                                            );
                                            continue;
                                        };
                                        let Some(ticket_in_id) = data.ticket_in_id else {
                                            tracing::error!(
                                                hub_id,
                                                "[Kafka] consumer CHECKIN_HUB_INFO (direct payload) missing ticket_in_id, skip record"
                                            );
                                            continue;
                                        };
                                        tracing::info!(
                                            ticket_in_id,
                                            hub_id,
                                            "[Kafka] consumer parsed direct data payload (no envelope)"
                                        );
                                        let repo_transport = Arc::clone(&repo_transport);
                                        let handle = tokio::spawn(async move {
                                            let r_transport =
                                                tokio::task::spawn_blocking(move || {
                                                    repo_transport
                                                        .update_hub_id_by_transport_trans_id(
                                                            ticket_in_id,
                                                            hub_id,
                                                        )
                                                })
                                                .await
                                                .unwrap_or_else(|e| {
                                                    Err(DbError::ExecutionError(e.to_string()))
                                                });
                                            let ok_transport = r_transport.is_ok();
                                            let updated_etdr = get_etdr_cache()
                                                .update_hub_id_by_ticket_id(ticket_in_id, hub_id);
                                            if ok_transport || updated_etdr {
                                                tracing::info!(
                                                    ticket_in_id,
                                                    hub_id,
                                                    updated_transport = ok_transport,
                                                    updated_etdr = updated_etdr,
                                                    "[Kafka] consumer updated HUB_ID by ticket_in_id from direct payload (TRANSPORT_TRANS_ID/ticket_id)"
                                                );
                                            }
                                            if let Err(e) = r_transport {
                                                tracing::warn!(ticket_in_id, hub_id, error = %e, "[Kafka] consumer TRANSPORT_TRANSACTION_STAGE update failed");
                                            }
                                        });
                                        handles.push(handle);
                                    }
                                    Err(e_standard) => {
                                        if let Ok(json_str) = std::str::from_utf8(value) {
                                            tracing::warn!(
                                                offset = record_and_offset.offset,
                                                bytes = value.len(),
                                                error = %e_standard,
                                                message_preview = %json_str.chars().take(200).collect::<String>(),
                                                "[Kafka] consumer parse JSON failed (envelope and direct formats)"
                                            );
                                        } else {
                                            tracing::warn!(
                                                offset = record_and_offset.offset,
                                                bytes = value.len(),
                                                error = %e_standard,
                                                "[Kafka] consumer parse JSON failed (invalid UTF-8)"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let processed_count = handles.len();
        for h in handles {
            if let Err(e) = h.await {
                tracing::warn!(partition_id, error = %e, "[Kafka] consumer task join error");
            }
        }

        next_offset = last_offset + 1;
        if let Err(e) = save_offset_to_file(offset_file.as_path(), next_offset) {
            tracing::warn!(partition_id, offset = next_offset, error = %e, "[Kafka] consumer save offset file failed");
        } else {
            tracing::debug!(
                partition_id,
                offset = next_offset,
                processed_count,
                "[Kafka] consumer saved offset after processing batch"
            );
        }
    }
}

/// Chạy vòng lặp consumer: detect partitions, spawn tasks để consume từ tất cả partitions.
/// Nếu keydb_opt = Some (HA): chỉ chạy consumer khi giành được leader lock trong KeyDB; renew lock định kỳ.
pub async fn run_checkin_hub_consumer(
    config: KafkaConfig,
    offset_file_path: Option<std::path::PathBuf>,
    keydb_opt: Option<std::sync::Arc<KeyDB>>,
) {
    if let Some(ref keydb) = keydb_opt {
        let leader_id = std::env::var("HOSTNAME").unwrap_or_else(|_| {
            std::env::var("POD_NAME").unwrap_or_else(|_| uuid::Uuid::new_v4().to_string())
        });
        let lock_key = config.consumer_leader_lock_key.as_str();
        let ttl = config.consumer_leader_lock_ttl_secs;
        loop {
            match keydb.set_nx_ex(lock_key, &leader_id, ttl).await {
                Ok(true) => {
                    tracing::info!(
                        leader_id = %leader_id,
                        lock_key = %lock_key,
                        ttl_secs = ttl,
                        "[Kafka] consumer leader lock acquired"
                    );
                    let keydb_renew = keydb.clone();
                    let lock_key_renew = lock_key.to_string();
                    let leader_id_renew = leader_id.clone();
                    let ttl_renew = ttl;
                    tokio::spawn(async move {
                        let mut interval =
                            tokio::time::interval(Duration::from_secs(ttl_renew / 2));
                        loop {
                            interval.tick().await;
                            if let Err(e) = keydb_renew
                                .set_ex(&lock_key_renew, &leader_id_renew, ttl_renew)
                                .await
                            {
                                tracing::warn!(lock_key = %lock_key_renew, error = %e, "[Kafka] consumer leader lock renew failed");
                            }
                        }
                    });
                    break;
                }
                Ok(false) => {
                    tracing::debug!(leader_id = %leader_id, "[Kafka] consumer not leader, waiting");
                }
                Err(e) => {
                    tracing::warn!(lock_key = %lock_key, error = %e, "[Kafka] consumer leader lock acquire failed, retrying");
                }
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }

    let topic = config.topic_checkin_hub_online.clone();
    let base_path = offset_file_path
        .unwrap_or_else(|| std::path::PathBuf::from(".kafka_checkin_hub_consumer_offset"));

    let client = match build_consumer_client(&config).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "[Kafka] consumer client build failed");
            return;
        }
    };

    let client = Arc::new(RwLock::new(client));

    // Chỉ lấy danh sách partition hợp lệ: từ config (0..n) hoặc từ metadata (list_topics), không probe partition không tồn tại.
    let valid_partitions: Vec<i32> = if let Some(n) = config.topic_checkin_partition_count {
        tracing::info!(topic = %topic, partition_count = n, "[Kafka] consumer using configured partition count");
        (0..n).collect()
    } else {
        match get_valid_partitions(Arc::clone(&client), &topic).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(topic = %topic, error = %e, "[Kafka] consumer failed to get valid partitions");
                return;
            }
        }
    };

    if valid_partitions.is_empty() {
        tracing::error!(topic = %topic, "[Kafka] consumer topic has no partitions");
        return;
    }

    tracing::info!(
        topic = %topic,
        partition_count = valid_partitions.len(),
        partitions = ?valid_partitions,
        "[Kafka] consumer starting for valid partitions only"
    );

    // Spawn một task cho mỗi partition hợp lệ
    for partition_id in &valid_partitions {
        let client_clone = Arc::clone(&client);
        let topic_clone = topic.clone();
        let base_path_clone = base_path.clone();
        let config_clone = config.clone();

        tracing::debug!(
            topic = %topic,
            partition_id = *partition_id,
            "[Kafka] consumer spawning task for partition"
        );

        let pid = *partition_id;
        tokio::spawn(async move {
            consume_partition(
                client_clone,
                topic_clone,
                pid,
                base_path_clone,
                config_clone,
            )
            .await;
        });
    }

    tracing::debug!(
        topic = %topic,
        partition_count = valid_partitions.len(),
        "[Kafka] consumer all partition tasks spawned"
    );

    // Task định kỳ re-detect valid partitions (chỉ khi không set partition count cố định)
    let client_for_recheck = Arc::clone(&client);
    let topic_for_recheck = topic.clone();
    let base_path_for_recheck = base_path.clone();
    let config_for_recheck = config.clone();
    let fixed_partition_count = config.topic_checkin_partition_count;
    let mut last_valid_partitions = valid_partitions.clone();
    const PARTITION_RECHECK_INTERVAL_SECS: u64 = 300; // Re-check mỗi 5 phút

    tokio::spawn(async move {
        if fixed_partition_count.is_some() {
            return; // Partition count cố định, không cần re-detect
        }
        loop {
            tokio::time::sleep(Duration::from_secs(PARTITION_RECHECK_INTERVAL_SECS)).await;

            let new_valid =
                match get_valid_partitions(Arc::clone(&client_for_recheck), &topic_for_recheck)
                    .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(
                            topic = %topic_for_recheck,
                            error = %e,
                            "[Kafka] consumer partition re-check failed"
                        );
                        continue;
                    }
                };

            let old_set: std::collections::HashSet<i32> =
                last_valid_partitions.iter().copied().collect();
            let new_set: std::collections::HashSet<i32> = new_valid.iter().copied().collect();
            if new_set != old_set {
                let added: Vec<i32> = new_valid
                    .iter()
                    .filter(|p| !old_set.contains(p))
                    .copied()
                    .collect();
                let removed: Vec<i32> = last_valid_partitions
                    .iter()
                    .filter(|p| !new_set.contains(p))
                    .copied()
                    .collect();
                tracing::info!(
                    topic = %topic_for_recheck,
                    added = ?added,
                    removed = ?removed,
                    "[Kafka] consumer detected partition change"
                );
                for partition_id in added {
                    let client_clone = Arc::clone(&client_for_recheck);
                    let topic_clone = topic_for_recheck.clone();
                    let base_path_clone = base_path_for_recheck.clone();
                    let config_clone = config_for_recheck.clone();
                    tokio::spawn(async move {
                        consume_partition(
                            client_clone,
                            topic_clone,
                            partition_id,
                            base_path_clone,
                            config_clone,
                        )
                        .await;
                    });
                }
                if !removed.is_empty() {
                    tracing::warn!(
                        topic = %topic_for_recheck,
                        removed = ?removed,
                        "[Kafka] consumer partition removal (existing tasks will handle errors)"
                    );
                }
                last_valid_partitions = new_valid;
            }
        }
    });

    // Giữ function chạy vô hạn (các partition consumers đã được spawn)
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}
