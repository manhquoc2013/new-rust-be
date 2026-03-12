//! Logging: file per process type, rotation by size, cleanup of old logs.
//! Dir layout: log_dir/YYYY-MM-DD/{name}-{type}.log — grep by day: `grep pattern logs/2025-02-28/*.log`.
#![allow(warnings)]

pub mod cleanup;
pub mod common;
pub mod config;
pub mod daily_rotating_writer;
pub mod file_logger;
pub mod process_type;
pub mod statistics;

pub use cleanup::{start_log_cleanup_task, CleanupConfig};
pub use config::{get_logging_config_from_env, LoggingConfig};
pub use file_logger::{clear_process_type, init_logging, set_process_type};
pub use process_type::{with_process_type_async, ProcessType, ProcessTypeGuard};
pub use statistics::{init_statistics, log_db_retry, log_db_time};
