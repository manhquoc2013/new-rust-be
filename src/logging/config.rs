//! Logging configuration: directory, file name, rotation size, retention, level, debug file.

use std::path::PathBuf;

/// Logging config: directory, file name, rotation size, retention, level.
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub log_dir: PathBuf,
    pub log_file_name: String,
    pub max_file_size: u64,
    pub retention_days: u64,
    pub log_level: String,
    /// When true: create *-debug-*.log and write all debug-level logs (from env LOG_DEBUG_FILE).
    pub enable_debug_file: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            log_dir: PathBuf::from("./logs"),
            log_file_name: String::from("rct"),
            max_file_size: 5 * 1024 * 1024, // 5MB
            retention_days: 7,
            log_level: String::from("info"),
            enable_debug_file: false,
        }
    }
}

/// Read logging config from environment. LOG_DEBUG_FILE=1|true|yes enables *-debug-*.log.
pub fn get_logging_config_from_env() -> LoggingConfig {
    use std::env;

    let log_dir = env::var("LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./logs"));

    let log_file_name = env::var("LOG_FILE_NAME").unwrap_or_else(|_| String::from("rct"));

    let max_file_size = env::var("LOG_MAX_FILE_SIZE")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(5 * 1024 * 1024);

    let retention_days = env::var("LOG_RETENTION_DAYS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(7);

    let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| String::from("info"));

    let enable_debug_file =
        crate::utils::parse_env_bool_loose(env::var("LOG_DEBUG_FILE").ok().as_deref());

    LoggingConfig {
        log_dir,
        log_file_name,
        max_file_size,
        retention_days,
        log_level,
        enable_debug_file,
    }
}
