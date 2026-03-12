//! Service tạo ticket_id dùng chung cho BECT và BOO.
//!
//! Luồng từ DB: lấy sequence 1 lần làm prefix (NEXTVAL duy nhất), biến đếm 1..suffix_max (ENV).
//! Khi đếm đến prefetch_threshold thì lấy tiếp 1 sequence; khi đếm đủ suffix_max thì reset về 1 và dùng sequence mới làm prefix.
//! Nếu lấy sequence từ DB failed → dùng generate_ticket_id từ utils/ticket_id.rs.
//! Không kiểm tra ticket trùng khi dùng id từ block (prefix + counter).

use crate::constants::ticket_id::{self as ticket_id_constants, MAX_RETRIES, RETRY_DELAY_MS};
use crate::db::repositories::TransportTransactionStageRepository;
use crate::db::Repository;
use crate::utils::generate_ticket_id_async;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

/// Singleton TicketIdService để chia sẻ state prefix/counter giữa mọi lời gọi.
static INSTANCE: Lazy<Arc<TicketIdService>> = Lazy::new(|| Arc::new(TicketIdService::new()));

/// Đảm bảo không trả về 0 (tránh transport_trans_id = 0 gây lỗi insert/update).
async fn ensure_non_zero_ticket_id(id: i64) -> i64 {
    if id != 0 {
        return id;
    }
    for _ in 0..3 {
        let x = generate_ticket_id_async().await;
        if x != 0 {
            return x;
        }
    }
    crate::utils::timestamp_ms().max(1)
}

/// Service lấy ticket_id cho checkin (BECT).
pub struct TicketIdService {
    bect_repo: Arc<TransportTransactionStageRepository>,
    /// Prefix hiện tại (seq từ DB). 0 = chưa khởi tạo.
    current_prefix: AtomicI64,
    /// Suffix atomic: 0..(suffix_max-1). Giá trị trả về = prefix * suffix_multiplier() + (suffix + 1).
    counter: AtomicI64,
    /// Seq tiếp theo đã prefetch (khi suffix >= prefetch_threshold). 0 = chưa có.
    next_prefix: AtomicI64,
    /// Đã gửi yêu cầu prefetch cho block hiện tại (tránh gọi nhiều lần).
    prefetch_requested: AtomicBool,
}

impl TicketIdService {
    pub fn new() -> Self {
        Self {
            bect_repo: Arc::new(TransportTransactionStageRepository::new()),
            current_prefix: AtomicI64::new(0),
            counter: AtomicI64::new(0),
            next_prefix: AtomicI64::new(0),
            prefetch_requested: AtomicBool::new(false),
        }
    }

    /// Trả về instance dùng chung (singleton).
    pub fn instance() -> Arc<Self> {
        Arc::clone(&INSTANCE)
    }

    /// Lấy ticket_id cho checkin (BECT/BOO): seq từ DB (1 lần) + suffix atomic 1..suffix_max (ENV).
    /// Id từ block không kiểm tra trùng; nếu DB failed dùng generate_ticket_id (utils/ticket_id).
    pub async fn get_next_ticket_id_for_checkin(&self, request_id: i64) -> i64 {
        for attempt in 0..=MAX_RETRIES {
            let (id, _from_block) = self.get_next_ticket_id_once(request_id).await;
            if id != 0 {
                return id;
            }
            if attempt < MAX_RETRIES {
                tracing::warn!(
                    request_id,
                    attempt = attempt + 1,
                    max = MAX_RETRIES,
                    "[DB] ticket_id=0 (DB failed), retry"
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(RETRY_DELAY_MS)).await;
            } else {
                tracing::warn!(
                    request_id,
                    "[DB] ticket_id=0 after retries, using generate_ticket_id"
                );
                return ensure_non_zero_ticket_id(generate_ticket_id_async().await).await;
            }
        }
        ensure_non_zero_ticket_id(generate_ticket_id_async().await).await
    }

    /// Gọi DB lấy NEXTVAL từ TRANSPORT_TRANS_STAGE_SEQ (sequence duy nhất).
    async fn fetch_next_seq_from_db(&self) -> i64 {
        let repo = self.bect_repo.clone();
        match tokio::task::spawn_blocking(move || repo.get_next_ticket_id()).await {
            Ok(Ok(s)) if s != 0 => s,
            Ok(Ok(_)) => {
                tracing::warn!("[DB] get_next_ticket_id returned 0");
                0
            }
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "[DB] get_next_ticket_id failed");
                0
            }
            Err(e) => {
                tracing::warn!(error = %e, "[DB] ticket_id spawn_blocking failed");
                0
            }
        }
    }

    /// Lấy ticket_id một lần: prefix (seq 1 lần) + suffix atomic 1..suffix_max; tại prefetch_threshold prefetch seq tiếp; tại suffix_max reset về 1 và dùng seq mới.
    /// Trả về (id, from_block): id luôn có chữ số cuối là 1 (ví dụ 5214 -> 52141); from_block=true khi id từ prefix+counter.
    async fn get_next_ticket_id_once(&self, request_id: i64) -> (i64, bool) {
        /// Nối chữ số 1 vào cuối id (5214 -> 52141). Tránh overflow bằng checked.
        fn append_last_digit_1(id: i64) -> i64 {
            id.checked_mul(10)
                .and_then(|x| x.checked_add(1))
                .unwrap_or(id)
        }
        let prefix = self.current_prefix.load(Ordering::Relaxed);
        if prefix == 0 {
            let seq = self.fetch_next_seq_from_db().await;
            if seq == 0 {
                let id = generate_ticket_id_async().await;
                tracing::debug!(
                    request_id,
                    "[DB] ticket_id from generate_ticket_id (DB failed)"
                );
                return (append_last_digit_1(id), false);
            }
            self.current_prefix.store(seq, Ordering::Relaxed);
            self.counter.store(1, Ordering::Relaxed);
            let mult = ticket_id_constants::suffix_multiplier();
            let id = seq * mult + 1;
            tracing::debug!(
                request_id,
                ticket_id = id,
                prefix = seq,
                "[DB] ticket_id from seq block (first), suffix=1"
            );
            self.maybe_trigger_prefetch(1).await;
            return (append_last_digit_1(id), true);
        }

        let c = self.counter.fetch_add(1, Ordering::Relaxed);
        let suffix = c + 1;

        if suffix > ticket_id_constants::suffix_max() {
            let next = self.wait_or_fetch_next_prefix().await;
            if next == 0 {
                let id = generate_ticket_id_async().await;
                tracing::debug!(
                    request_id,
                    "[DB] ticket_id from generate_ticket_id (next_prefix failed)"
                );
                return (append_last_digit_1(id), false);
            }
            self.current_prefix.store(next, Ordering::Relaxed);
            self.next_prefix.store(0, Ordering::Relaxed);
            self.counter.store(1, Ordering::Relaxed);
            self.prefetch_requested.store(false, Ordering::Relaxed);
            let mult = ticket_id_constants::suffix_multiplier();
            let id = next * mult + 1;
            tracing::debug!(
                request_id,
                ticket_id = id,
                prefix = next,
                "[DB] ticket_id new block, suffix=1"
            );
            self.maybe_trigger_prefetch(1).await;
            return (append_last_digit_1(id), true);
        }

        self.maybe_trigger_prefetch(suffix).await;
        let mult = ticket_id_constants::suffix_multiplier();
        let id = prefix * mult + suffix;
        let max = ticket_id_constants::suffix_max();
        let thresh = ticket_id_constants::prefetch_threshold();
        if suffix == 2 || suffix == thresh || suffix == max {
            tracing::debug!(
                request_id,
                ticket_id = id,
                prefix,
                suffix,
                "[DB] ticket_id from seq block"
            );
        }
        (append_last_digit_1(id), true)
    }

    /// Gọi prefetch khi suffix >= prefetch_threshold() (chỉ một lần mỗi block). Gọi DB qua spawn_blocking và set next_prefix.
    async fn maybe_trigger_prefetch(&self, suffix: i64) {
        if suffix < ticket_id_constants::prefetch_threshold() {
            return;
        }
        if self
            .prefetch_requested
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        let repo = self.bect_repo.clone();
        let seq = tokio::task::spawn_blocking(move || repo.get_next_ticket_id()).await;
        if let Ok(Ok(s)) = seq {
            if s != 0 {
                self.next_prefix.store(s, Ordering::Relaxed);
            }
        }
    }

    /// Khi counter vượt suffix_max cần prefix mới: đợi next_prefix được set hoặc fetch sync.
    async fn wait_or_fetch_next_prefix(&self) -> i64 {
        for _ in 0..50 {
            let next = self.next_prefix.load(Ordering::Relaxed);
            if next != 0 {
                return next;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        self.fetch_next_seq_from_db().await
    }
}

impl Default for TicketIdService {
    fn default() -> Self {
        Self::new()
    }
}
