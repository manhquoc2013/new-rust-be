//! Tiện ích: format hex, wrap encrypted, generate request_id, generate ticket_id, epoch datetime.

pub mod epoch_datetime;
pub mod helpers;
pub mod host_ip;
pub mod ip_denylist;
pub mod ticket_id;
pub mod toll_filter;

pub use toll_filter::is_toll_id_valid;

pub use epoch_datetime::{
    epoch_to_datetime_utc,
    format_datetime_utc_db, now_utc_db_string, now_utc_naive, timestamp_ms,
};
pub use helpers::{
    bect_skip_account_check, fe_status_detail, 
    log_trace_ids, normalize_etag, parse_env_bool_loose, wrap_encrypted_reply,
};
pub use ip_denylist::CompiledIpDenylist;
pub use ticket_id::generate_ticket_id_async;
