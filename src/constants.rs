//! Command_id constants for FE, VDTC, VETC protocols (avoid magic numbers in code).
//!
//! **Luồng FE:** Các cặp bản tin req/resp đều từ FE: FE gửi req (CONNECT, HANDSHAKE, CHECKIN, COMMIT, ROLLBACK, TERMINATE), backend trả resp tương ứng (CONNECT_RESP, SHAKE_RESP, CHECKIN_RESP, COMMIT_RESP, ROLLBACK_RESP, TERMINATE_RESP). Processor phân command_id và gọi handler tương ứng.

/// Front-End (FE) protocol command IDs – cặp req/resp từ FE.
pub mod fe {
    /// CONNECT – FE gửi req (44 bytes). Backend trả CONNECT_RESP.
    pub const CONNECT: i32 = 0x6C;

    /// CONNECT_RESP – Backend trả FE sau khi xử lý CONNECT (32 bytes).
    pub const CONNECT_RESP: i32 = 0x6D;

    /// HANDSHAKE (SHAKE) – FE gửi req (28 bytes). Backend trả SHAKE_RESP.
    pub const HANDSHAKE: i32 = 0x1C;

    /// SHAKE_RESP – Backend trả FE sau khi xử lý HANDSHAKE (32 bytes).
    pub const SHAKE_RESP: i32 = 0x6F;

    /// CHECKIN – FE gửi req (172 bytes). Backend trả CHECKIN_RESP.
    pub const CHECKIN: i32 = 0x66;

    /// CHECKIN_RESP – Backend trả FE sau khi xử lý CHECKIN (76 bytes).
    pub const CHECKIN_RESP: i32 = 0x67;

    /// COMMIT – FE gửi req (152 bytes, spec 3A). Backend trả COMMIT_RESP. Processor gọi handle_commit.
    pub const COMMIT: i32 = 0x68;

    /// COMMIT_RESP – Backend trả FE sau khi xử lý COMMIT (84 bytes, spec 3B).
    pub const COMMIT_RESP: i32 = 0x69;

    /// ROLLBACK – FE gửi req (152 bytes, spec 3A CHECKIN_ROLLBACK_BOO). Backend trả ROLLBACK_RESP. Processor gọi handle_rollback.
    pub const ROLLBACK: i32 = 0x6A;

    /// ROLLBACK_RESP – Backend trả FE sau khi xử lý ROLLBACK (84 bytes, spec 3B CHECKIN_ROLLBACK_BOO_RESP).
    pub const ROLLBACK_RESP: i32 = 0x6B;

    /// TERMINATE (0E) – FE gửi req (28 bytes, spec 2.3.1.7.11). Backend trả TERMINATE_RESP.
    pub const TERMINATE: i32 = 0x70;

    /// TERMINATE_RESP (0F) – Backend trả FE sau khi xử lý TERMINATE (32 bytes, spec 2.3.1.7.12).
    pub const TERMINATE_RESP: i32 = 0x71;

    /// QUERY_VEHICLE_BOO command - Client sends for query vehicle boo - Msg length: 122 bytes
    pub const QUERY_VEHICLE_BOO: i32 = 0x64;
    /// QUERY_VEHICLE_BOO_RESP - Server response to QUERY_VEHICLE_BOO - Msg length: 133 bytes
    pub const QUERY_VEHICLE_BOO_RESP: i32 = 0x65;

    /// LOOKUP_VEHICLE command - Client sends for lookup vehicle - Msg length: 110 bytes
    pub const LOOKUP_VEHICLE: i32 = 0x96;
    /// LOOKUP_VEHICLE_RESP - Server response to LOOKUP_VEHICLE - Msg length: 197 bytes
    pub const LOOKUP_VEHICLE_RESP: i32 = 0x97;

    /// CHECKOUT_RESERVE_BOO command - Client sends for checkout reserve boo - Msg length: total length of the message
    pub const CHECKOUT_RESERVE_BOO: i32 = 0x98;
    /// CHECKOUT_RESERVE_BOO_RESP - Server response to CHECKOUT_RESERVE_BOO - Msg length: 100 bytes
    pub const CHECKOUT_RESERVE_BOO_RESP: i32 = 0x99;

    /// CHECKOUT_COMMIT_BOO command - Client sends for checkout commit boo - Msg length: 178 bytes
    pub const CHECKOUT_COMMIT_BOO: i32 = 0x9A;
    /// CHECKOUT_COMMIT_BOO_RESP - Server response to CHECKOUT_COMMIT_BOO - Msg length: 96 bytes
    pub const CHECKOUT_COMMIT_BOO_RESP: i32 = 0x9B;

    /// CHECKOUT_ROLLBACK_BOO command - Client sends for checkout rollback boo - Msg length: 178 bytes
    pub const CHECKOUT_ROLLBACK_BOO: i32 = 0x9C;
    /// CHECKOUT_ROLLBACK_BOO_RESP - Server response to CHECKOUT_ROLLBACK_BOO - Msg length: 96 bytes
    pub const CHECKOUT_ROLLBACK_BOO_RESP: i32 = 0x9D;

    /// BOO 3A CHECKIN_COMMIT_BOO (same value as COMMIT). Spec 2.3.1.7.7.
    pub const CHECKIN_COMMIT_BOO: i32 = 0x68;
    /// BOO 3B CHECKIN_COMMIT_BOO_RESP (same value as COMMIT_RESP). Spec 2.3.1.7.8.
    pub const CHECKIN_COMMIT_BOO_RESP: i32 = 0x69;

    /// BOO 3A CHECKIN_ROLLBACK_BOO (same value as ROLLBACK). Spec 2.3.1.7.9.
    pub const CHECKIN_ROLLBACK_BOO: i32 = 0x6A;
    /// BOO 3B CHECKIN_ROLLBACK_BOO_RESP (same value as ROLLBACK_RESP). Spec 2.3.1.7.10.
    pub const CHECKIN_ROLLBACK_BOO_RESP: i32 = 0x6B;

    /// FE status: 0=Success, 14..107=subscriber/account/etag, 200..304=transaction/route/price, 901..903=system/BOO integration.
    /// 301 NOT_FOUND_STATION_LANE - Lane not found for station; CONNECT: used for unauthorized (wrong password)
    pub const NOT_FOUND_STATION_LANE: i32 = 301;
    /// 302 NOT_FOUND_STAGE - Stage/route not found (closed station)
    #[allow(dead_code)]
    pub const NOT_FOUND_STAGE: i32 = 302;
    /// 303 NOT_FOUND_PRICE_INFO - Price info not found for station or stage
    pub const NOT_FOUND_PRICE_INFO: i32 = 303;
    /// 304 PRICE_FORMAT_INCORRECT - Price format invalid
    #[allow(dead_code)]
    pub const PRICE_FORMAT_INCORRECT: i32 = 304;
    /// 305 ACCOUNT_NOT_ACTIVATED - Account not activated (TCOC_USERS.STATUS != '1')
    pub const ACCOUNT_NOT_ACTIVATED: i32 = 305;
    /// 306 USER_NOT_FOUND - User not found (username/toll_id) on CONNECT
    pub const USER_NOT_FOUND: i32 = 306;
    /// 14 STATUS_COMMAND_ID_INVALID - commandId invalid
    #[allow(dead_code)]
    pub const STATUS_COMMAND_ID_INVALID: i32 = 14;
    /// 200 NOT_FOUND_TOLL_TRANSACTION - CHECKIN transaction not found (on COMMIT)
    #[allow(dead_code)]
    pub const NOT_FOUND_TOLL_TRANSACTION: i32 = 200;
    /// 201 NOT_FOUND_LANE_TRANSACTION - Lane transaction not found (commit/rollback)
    #[allow(dead_code)]
    pub const NOT_FOUND_LANE_TRANSACTION: i32 = 201;
    /// 202 NOT_FOUND_ROUTE_TRANSACTION - Route transaction not found (CHECKIN at exit station / already checkout)
    pub const NOT_FOUND_ROUTE_TRANSACTION: i32 = 202;
    /// 203 CHECKIN_SESSION_TIMEOUT - CHECKIN session expired (closed station)
    #[allow(dead_code)]
    pub const CHECKIN_SESSION_TIMEOUT: i32 = 203;
    /// 211 VERIFY_DB_FAILED - Could not verify record after DB write (ensures fe_resp success only when data matches DB)
    pub const VERIFY_DB_FAILED: i32 = 211;
    /// 212 SAVE_DB_ERROR - DB write error
    pub const SAVE_DB_ERROR: i32 = 212;
    /// 213 DUPLICATE_TRANSACTION - Another record with same ticket_in_id exists with different transport_trans_id (trùng mã giao dịch)
    pub const DUPLICATE_TRANSACTION: i32 = 213;
    /// 901 BOO_1_ERROR - VETC (BOO1) system error: timeout / parse error / send error / connection lost / session not ready / no client.
    pub const BOO_1_ERROR: i32 = 901;
    /// 902 BOO_2_ERROR - VDTC (BOO2) system error: timeout / parse error / send error / connection lost / session not ready / no client.
    pub const BOO_2_ERROR: i32 = 902;
    /// 903 ERROR_REASON_SYSTEM - Non-BOO system error (DB/service/internal failures).
    pub const ERROR_REASON_SYSTEM: i32 = 903;
    /// 16 FE_MESSAGE_FORMAT_4BYTE_REJECTED - Only accept FE 8-byte messages (request_id/session_id i64); 4-byte messages rejected when FE_REQUIRE_8BYTE_IDS is set
    pub const FE_MESSAGE_FORMAT_4BYTE_REJECTED: i32 = 16;
}

/// List of valid Front-End command IDs. Used in processor to validate incoming request.
pub const FE_VALID_COMMANDS: &[i32] = &[
    fe::CONNECT,         // 0x6C
    fe::HANDSHAKE,       // 0x1C
    fe::CHECKIN,         // 0x66
    fe::COMMIT,          // 0x68
    fe::ROLLBACK,        // 0x6A
    fe::TERMINATE,       // 0x70
    fe::QUERY_VEHICLE_BOO, // 0x64 (1A)
    fe::CHECKOUT_RESERVE_BOO, // 0x98 (2AZ)
];

/// Returns response command_id for the given request command_id (FE protocol).
#[inline(always)]
pub fn fe_response_command_id(command_id: i32) -> i32 {
    match command_id {
        fe::CONNECT => fe::CONNECT_RESP,
        fe::HANDSHAKE => fe::SHAKE_RESP,
        fe::TERMINATE => fe::TERMINATE_RESP,
        fe::CHECKIN => fe::CHECKIN_RESP,
        fe::COMMIT => fe::COMMIT_RESP,
        fe::ROLLBACK => fe::ROLLBACK_RESP,
        fe::QUERY_VEHICLE_BOO => fe::QUERY_VEHICLE_BOO_RESP,
        fe::CHECKOUT_RESERVE_BOO => fe::CHECKOUT_RESERVE_BOO_RESP,
        _ => fe::CONNECT_RESP,
    }
}

/// Compare command_id with FE command list.
#[allow(dead_code)]
pub fn is_fe_command(cmd_id: i32) -> bool {
    matches!(
        cmd_id,
        fe::CONNECT
            | fe::CONNECT_RESP
            | fe::HANDSHAKE
            | fe::SHAKE_RESP
            | fe::CHECKIN
            | fe::CHECKIN_RESP
            | fe::COMMIT
            | fe::COMMIT_RESP
            | fe::ROLLBACK
            | fe::ROLLBACK_RESP
            | fe::TERMINATE
            | fe::TERMINATE_RESP
            | fe::QUERY_VEHICLE_BOO
            | fe::QUERY_VEHICLE_BOO_RESP
            | fe::CHECKOUT_RESERVE_BOO
            | fe::CHECKOUT_RESERVE_BOO_RESP
    )
}

/// Returns command name (string) for command_id.
#[allow(dead_code)]
#[allow(unreachable_patterns)]
pub fn get_command_name(cmd_id: i32) -> &'static str {
    match cmd_id {
        fe::CONNECT => "FE_CONNECT",
        fe::CONNECT_RESP => "FE_CONNECT_RESP",
        fe::HANDSHAKE => "FE_HANDSHAKE",
        fe::SHAKE_RESP => "FE_SHAKE_RESP",
        fe::CHECKIN => "FE_CHECKIN", // Same value as vdtc::CHECKIN (0x04)
        fe::CHECKIN_RESP => "FE_CHECKIN_RESP",
        fe::COMMIT => "FE_COMMIT", // Same as BOO CHECKIN_COMMIT_BOO (3A, 0x68)
        fe::COMMIT_RESP => "FE_COMMIT_RESP", // Same as BOO CHECKIN_COMMIT_BOO_RESP (3B, 0x69)
        fe::ROLLBACK => "FE_ROLLBACK", // Same as BOO CHECKIN_ROLLBACK_BOO (3A, 0x6A)
        fe::ROLLBACK_RESP => "FE_ROLLBACK_RESP", // Same as BOO CHECKIN_ROLLBACK_BOO_RESP (3B, 0x6B)
        fe::TERMINATE => "FE_TERMINATE",
        fe::TERMINATE_RESP => "FE_TERMINATE_RESP",
        fe::QUERY_VEHICLE_BOO => "FE_QUERY_VEHICLE_BOO",
        fe::QUERY_VEHICLE_BOO_RESP => "FE_QUERY_VEHICLE_BOO_RESP",
        fe::CHECKOUT_RESERVE_BOO => "FE_CHECKOUT_RESERVE_BOO",       // 2AZ, 0x98
        fe::CHECKOUT_RESERVE_BOO_RESP => "FE_CHECKOUT_RESERVE_BOO_RESP", // 2BZ, 0x99
        _ => "UNKNOWN_COMMAND",
    }
}

/// Network / BOO client constants
pub mod network {
    /// Max bytes to format as hex in debug logs when capping (optional; currently log full for trace).
    #[allow(dead_code)]
    pub const MAX_DEBUG_HEX_BYTES: usize = 512;

    /// Maximum size of one message (256KB), used for connection and BOO client.
    pub const MAX_MESSAGE_SIZE: usize = 262144;

    /// Maximum consecutive errors before clearing buffer / closing connection.
    pub const MAX_CONSECUTIVE_ERRORS: usize = 3;

    /// Plain-text "pong" response for Ping (4 bytes).
    pub const PONG_RESPONSE: &[u8] = b"pong";

    /// Max closed conn_ids to keep in router set (drop older when full to avoid unbounded growth).
    pub const MAX_CLOSED_CONN_IDS: usize = 10_000;
}

/// Lane type constants for TOLL_LANE
/// Used to determine lane type (entry or exit lane)
pub mod lane_type {
    /// Entry lane (IN) - Vehicle entering station
    pub const IN: &str = "1";

    /// Exit lane (OUT) - Vehicle leaving station
    pub const OUT: &str = "2";
}

/// BECT/FE error codes (per spec: 100..107 subscriber/account/etag/debit, 200..304 transaction/route/price).
#[allow(dead_code)]
pub mod bect {
    /// 100 SUBSCRIBER_NOT_FOUND - Subscriber not found
    pub const SUBSCRIBER_NOT_FOUND: i32 = 100;
    /// 101 SUBSCRIBER_IS_NOT_ACTIVE - Subscriber status invalid (not active)
    pub const SUBSCRIBER_IS_NOT_ACTIVE: i32 = 101;
    /// 102 ACCOUNT_IS_NOT_ACTIVE - Account invalid (not active)
    pub const ACCOUNT_IS_NOT_ACTIVE: i32 = 102;
    /// 103 ETAG_IS_NOT_ACTIVE - Etag invalid (not active state)
    pub const ETAG_IS_NOT_ACTIVE: i32 = 103;
    /// 104 ACCOUNT_NOT_ENOUGH_MONEY - Account has insufficient balance
    pub const ACCOUNT_NOT_ENOUGH_MONEY: i32 = 104;
    /// 105 ACCOUNT_NOT_FOUND - Account linked to ETAG not found
    pub const ACCOUNT_NOT_FOUND: i32 = 105;
    /// 106 DEBIT_AMOUNT_IS_OVER - Account debit exceeds allowed limit
    pub const DEBIT_AMOUNT_IS_OVER: i32 = 106;
    /// 107 DEBIT_IS_OVERTIME - Account debit past grace period
    pub const DEBIT_IS_OVERTIME: i32 = 107;
    /// 303 NOT_FOUND_PRICE_INFO - Price table not found (used when fee calculation fails)
    pub const NOT_FOUND_PRICE_INFO: i32 = 303;
}

/// Constants for ticket_id generation/retry in services (BOO/VETC/VDTC/BECT).
pub mod ticket_id {
    use once_cell::sync::Lazy;

    /// Number of retries when ticket_id = 0 (total attempts = MAX_RETRIES + 1).
    pub const MAX_RETRIES: u32 = 5;

    /// Delay (ms) between retries when ticket_id = 0.
    pub const RETRY_DELAY_MS: u64 = 20;

    /// Last digit (units) fixed for local-generated ticket_id (UUID hash).
    pub const LOCAL_LAST_DIGIT: u64 = 2;

    /// Suffix: 1..=suffix_max(). ticket_id = prefix * suffix_multiplier() + suffix.
    /// Number of suffix digits per TICKET_ID_SUFFIX_MAX (e.g. 999 → 3 digits, multiplier 1000).
    /// Configure via ENV: TICKET_ID_SUFFIX_MAX (default 999, clamp 1..999).
    static TICKET_ID_SUFFIX_MAX: Lazy<i64> = Lazy::new(|| {
        std::env::var("TICKET_ID_SUFFIX_MAX")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .map(|v| v.clamp(1, 9))
            .unwrap_or(9)
    });

    /// When suffix reaches this threshold, prefetch next sequence from DB (tens of numbers left).
    /// Configure via ENV: TICKET_ID_PREFETCH_THRESHOLD (default 980, clamp 1..=suffix_max).
    static TICKET_ID_PREFETCH_THRESHOLD: Lazy<i64> = Lazy::new(|| {
        let max = *TICKET_ID_SUFFIX_MAX;
        std::env::var("TICKET_ID_PREFETCH_THRESHOLD")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .map(|v| v.clamp(1, max))
            .unwrap_or_else(|| (max - 2).max(1))
    });

    /// Maximum suffix (1..=999, read from ENV TICKET_ID_SUFFIX_MAX).
    pub fn suffix_max() -> i64 {
        *TICKET_ID_SUFFIX_MAX
    }

    /// Multiplier to combine prefix + suffix: 10^(digit count of suffix_max). E.g. suffix_max=999 → 1000, 99 → 100.
    pub fn suffix_multiplier() -> i64 {
        let max = suffix_max();
        let digits = (max as f64).log10().floor() as u32 + 1;
        10_i64.pow(digits)
    }

    /// Prefetch next sequence threshold (read from ENV TICKET_ID_PREFETCH_THRESHOLD, not to exceed suffix_max).
    pub fn prefetch_threshold() -> i64 {
        *TICKET_ID_PREFETCH_THRESHOLD
    }
}

/// Retry constants when COMMIT fetches BOO/TRANSPORT record from DB (race with CHECKIN save).
pub mod commit_retry {
    /// Number of get_by_id retries (total attempts = MAX_RETRIES + 1).
    pub const MAX_RETRIES: u32 = 3;

    /// Initial delay (ms); linear backoff: delay * (retry_count + 1).
    pub const INITIAL_RETRY_DELAY_MS: u64 = 50;
}

/// Commit handler: max concurrent ETDR cleanup tasks (semaphore cap).
pub mod commit {
    /// Max concurrent ETDR cleanup + TCD invalidate tasks to avoid contention.
    pub const MAX_CONCURRENT_ETDR_CLEANUP: usize = 8;

    /// Retries when verifying BOO record from DB before clearing ETDR cache (avoid clear when DB save failed or not yet visible).
    pub const VERIFY_BEFORE_CLEANUP_MAX_RETRIES: u32 = 5;
    /// Delay (ms) between verify retries; linear backoff: delay * (attempt).
    pub const VERIFY_BEFORE_CLEANUP_DELAY_MS: u64 = 50;

    /// Allow ETDR cleanup without DB verify if more than this many ms since checkout_datetime (5 minutes).
    pub const ALLOW_CLEAR_AFTER_CHECKOUT_MS: u64 = 5 * 60 * 1000;
}

/// Verify-after-save constants for stage (CHECKIN) – keep for retry get_by_id if needed.
#[allow(dead_code)]
pub mod verify_after_save {
    pub const MAX_RETRIES: u32 = 3;
    pub const INITIAL_VERIFY_DELAY_MS: u64 = 50;
}

/// Verify after save in service (BOO / TRANSPORT_TRANSACTION_STAGE) – exponential backoff, more retries.
pub mod verify_after_save_service {
    pub const MAX_RETRIES: u32 = 5;
    pub const INITIAL_VERIFY_DELAY_MS: u64 = 50;
}

/// Pool constants (get_connection_with_retry).
pub mod pool {
    /// Number of retries when pool.get() fails.
    pub const GET_CONNECTION_MAX_RETRIES: u32 = 5;
    /// Initial backoff between retries (ms).
    pub const GET_CONNECTION_INITIAL_BACKOFF_MS: u64 = 1000;
    /// Backoff cap (ms).
    pub const GET_CONNECTION_MAX_BACKOFF_MS: u64 = 30_000;
}

/// ETDR constants (cache TTL, no-checkout max age, retry lock HA).
pub mod etdr {
    /// Max age (ms) of non-checkout record before cleanup (3 days).
    pub const NO_CHECKOUT_MAX_AGE_MS: i64 = 3 * 24 * 3600 * 1000;
    /// TTL (ms) cho ETDR cache.
    pub const ETDR_TTL_MS: i64 = 3_600_000;
    /// KeyDB lock key prefix for ETDR retry (HA: only one node processes each key per cycle).
    pub const ETDR_RETRY_LOCK_PREFIX: &str = "lock:etdr_retry:";
    /// KeyDB key prefix for ETDR retry store: one key per ticket_id (etdr_retry:{ticket_id}) so multiple transactions per etag are not overwritten.
    pub const ETDR_RETRY_KEY_PREFIX: &str = "etdr_retry:";
    /// TTL (seconds) for retry lock; large enough for one cycle to finish, avoid deadlock.
    pub const ETDR_RETRY_LOCK_TTL_SECS: u64 = 90;
    /// Min delay (ms) between processing two records when backlog exists, to avoid overloading KeyDB/DB.
    pub const ETDR_RETRY_BACKOFF_MS: u64 = 2000;
    /// After this many failed DB save attempts, remove ETDR from retries list (stop retrying).
    pub const ETDR_MAX_DB_SAVE_RETRIES: i32 = 3;
    /// Window (ms) when choosing best record: if checkIn(BOO/Transport) + window > checkIn(Sync) then take BOO/Transport record (avoid duplicate when sync is a few seconds late).
    pub const BEST_RECORD_BOO_SYNC_WINDOW_MS: i64 = 5_000;
    /// KeyDB lock key prefix for TCD save (one writer per transport_trans_id to avoid duplicate rows by toll_a/toll_b).
    pub const TCD_SAVE_LOCK_PREFIX: &str = "lock:tcd_save:";
    /// TTL (seconds) for TCD save lock; long enough for one save_tcd_list cycle.
    pub const TCD_SAVE_LOCK_TTL_SECS: u64 = 30;
    /// KeyDB key prefix for pending TCD (per transport_trans_id).
    pub const PENDING_TCD_KEY_PREFIX: &str = "pending_tcd:";
}

/// db_retry constants (buffer and flush when KeyDB is lost).
pub mod db_retry {
    /// Max records in fallback buffer for session (FIFO drop when full).
    pub const BUFFER_MAX_SESSIONS: usize = 500;
    /// Max records in fallback buffer for conn_server/user (per key).
    pub const BUFFER_MAX_BY_KEY: usize = 200;
    /// Number of DB write retry operations run in parallel per type.
    pub const RETRY_CONCURRENCY: usize = 8;
    /// Batch size when flushing buffer to KeyDB.
    pub const FLUSH_BATCH_SIZE: usize = 50;
}

/// TERMINATE_RESP status codes (FE).
pub mod terminate {
    #[allow(dead_code)]
    pub const SUCCESS: i32 = 0;
    #[allow(dead_code)]
    pub const ERROR_GENERAL: i32 = 1;
    pub const ERROR_NO_ENCRYPTION_KEY: i32 = 301;
    pub const ERROR_EMPTY_KEY: i32 = 302;
    pub const ERROR_SOCKET_CONVERSION: i32 = 303;
    pub const ERROR_DECRYPT_FAILED: i32 = 304;
    pub const ERROR_HANDSHAKE_TIMEOUT: i32 = 305;
    /// Invalid or broken message format (length header, too many consecutive parse errors).
    pub const ERROR_INVALID_MESSAGE: i32 = 306;
}

/// Cache constants (default TTL for service cache-aside).
pub mod cache {
    /// Default TTL (seconds) for entity cache (1 hour).
    pub const DEFAULT_CACHE_TTL_SECS: u64 = 3600;
    /// Max stale keys to remove per atomic_reload_prefix (CacheManager) to avoid long blocking.
    pub const ATOMIC_RELOAD_STALE_CAP: usize = 5_000;
    /// Default max keys per prefix op for Moka (invalidate_prefix / get_keys_with_prefix).
    pub const MOKA_PREFIX_OP_MAX_KEYS: usize = 10_000;
}

/// Sentinel / constants used in checkin (FE response plate).
pub mod checkin {
    /// Plate value when no plate (aligned with Java TCOCMessageChannel).
    pub const PLATE_EMPTY_SENTINEL: &str = "0000000000";
}

/// Numeric and DB code values for price_ticket_type (match Java TCOCConstants).
pub mod price_ticket_type {
    /// Numeric value for price_ticket_type.
    #[allow(dead_code)]
    pub mod value {
        pub const DEFAULT: i32 = 1;
        pub const LP_IN_VD3: i32 = 93;
        pub const BL_TOLL: i32 = 94;
        pub const BL_ALL: i32 = 95;
        pub const FREE_TURNING_SECOND: i32 = 96;
        pub const WL_TOLL: i32 = 97;
        pub const WL_ALL: i32 = 98;
        pub const FREE_TURNING: i32 = 99;
    }
    /// Corresponding DB storage string (code BL, W, ... or number).
    #[allow(dead_code)]
    pub mod db_code {
        pub const DEFAULT: &str = "0";
        pub const LP_IN_VD3: &str = "93";
        pub const BL_TOLL: &str = "BL";
        pub const BL_ALL: &str = "BL_ALL";
        pub const FREE_TURNING_SECOND: &str = "96";
        pub const WL_TOLL: &str = "W";
        pub const WL_ALL: &str = "W_ALL";
        pub const FREE_TURNING: &str = "99";
    }
}

/// Kafka producer service constants (event types, KeyDB keys, reconnect, batch).
pub mod kafka_producer {
    /// Event type for checkin payload to HUB.
    pub const EVENT_TYPE_CHECKIN: &str = "CHECKIN_INFO";
    /// Event type for checkout payload to HUB.
    pub const EVENT_TYPE_CHECKOUT: &str = "CHECKOUT_INFO";
    /// KeyDB list key for pending checkin when Kafka send fails.
    pub const KEYDB_LIST_CHECKIN: &str = "kafka_pending:checkin";
    /// KeyDB list key for pending checkout when Kafka send fails.
    pub const KEYDB_LIST_CHECKOUT: &str = "kafka_pending:checkout";
    /// Max list length per KeyDB pending list (trim oldest when exceeded).
    pub const KEYDB_PENDING_MAX_LIST_LEN: usize = 5_000;
    /// TTL (seconds) for KeyDB pending list keys (7 days).
    pub const KEYDB_PENDING_TTL_SECS: u64 = 7 * 24 * 3600;
    /// Interval (seconds) between replay cycles from KeyDB.
    pub const KEYDB_REPLAY_INTERVAL_SECS: u64 = 30;
    /// Max items replayed per list per cycle.
    pub const KEYDB_REPLAY_MAX_PER_CYCLE: usize = 200;
    /// LPOP batch size when replaying from KeyDB.
    pub const KEYDB_REPLAY_LPOP_BATCH: usize = 50;
    /// Consecutive timeouts before triggering reconnect.
    pub const RECONNECT_AFTER_CONSECUTIVE_TIMEOUTS: u32 = 3;
    /// Min gap (seconds) between two reconnect attempts.
    pub const RECONNECT_DEBOUNCE_SECS: u64 = 10;
    /// Timeout (seconds) for reconnect (build_client).
    pub const RECONNECT_TIMEOUT_SECS: u64 = 30;
    /// Total timeout (seconds) for one batch send (retry + backoff).
    pub const BATCH_SEND_TOTAL_TIMEOUT_SECS: u64 = 120;
    /// Cap partitions from metadata to avoid huge produce batches.
    pub const MAX_PARTITIONS_FROM_METADATA: i32 = 128;
}

/// Kafka consumer service constants (event type, fetch, poll).
pub mod kafka_consumer {
    /// Event type for checkin hub info from HUB.
    pub const EVENT_TYPE_CHECKIN_HUB_INFO: &str = "CHECKIN_HUB_INFO";
    /// Fetch min bytes per request.
    pub const FETCH_MIN_BYTES: i32 = 1;
    /// Fetch max bytes per request.
    pub const FETCH_MAX_BYTES: i32 = 1_000_000;
    /// Max wait (ms) when fetching.
    pub const FETCH_MAX_WAIT_MS: i32 = 5_000;
    /// Sleep (ms) between polls when idle.
    pub const POLL_IDLE_SLEEP_MS: u64 = 500;
}

/// Epoch (seconds vs milliseconds) threshold for datetime conversion.
pub mod epoch {
    /// Epoch >= this value is treated as milliseconds (e.g. 1e12 ms ≈ 2001-09-09).
    pub const MS_THRESHOLD: i64 = 1_000_000_000_000;
}

/// ACCOUNT_TRANSACTION table / domain constants (AUTOCOMMIT, AUTOCOMMIT_STATUS, CHARGE_IN_ETC).
pub mod account_transaction {
    /// AUTOCOMMIT = 1: autocommit (immediate commit).
    pub const AUTOCOMMIT_AUTO: i32 = 1;
    /// AUTOCOMMIT = 2: lock (reserve); commit will set AUTOCOMMIT_STATUS and FINISH_DATETIME.
    pub const AUTOCOMMIT_LOCK: i32 = 2;
    /// AUTOCOMMIT_STATUS: 1 = COMMIT success.
    pub const AUTOCOMMIT_STATUS_SUCCESS: i32 = 1;
    /// AUTOCOMMIT_STATUS: 2 = ROLLBACK / fail.
    pub const AUTOCOMMIT_STATUS_FAIL: i32 = 2;
    /// ACCOUNT_TRANS_TYPE for charge.
    pub const ACCOUNT_TRANS_TYPE_CHARGE: i32 = 2;
    /// CHARGE_IN value for ETC.
    pub const CHARGE_IN_ETC: &str = "E";
}
