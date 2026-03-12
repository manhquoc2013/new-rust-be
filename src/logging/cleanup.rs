//! Task to cleanup old logs by retention_days and interval.
//!
//! **Statistics:** Optional cleanup of `statistics/` by `statistics_retention_days` (e.g. 30–90); files deleted by mtime.
//!
//! **Disk full and performance:**
//! - When disk is full: log write fails (ENOSPC). App may keep running but new log lines can be lost (depends on writer error handling). After freeing space (manual delete or cleanup), new writes work again.
//! - Cleanup removes old log/statistics by retention to free disk; it does not run on disk-full (runs on interval). Monitor disk and run cleanup often enough (e.g. hourly).
//! - **Performance when cleanup runs:** Cleanup runs in `spawn_blocking` so the async runtime is not blocked. While it runs: read_dir, stat, remove_file increase disk I/O for a few seconds. Log writers and cleanup share the disk so write latency may briefly increase (a few ms to tens of ms). With retention 7–90 days, files per run are bounded; on SSD impact is usually small.

use chrono::{Duration, Local};
use std::fs;
use std::path::PathBuf;
use std::time;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Config for the log cleanup task.
#[derive(Debug, Clone)]
pub struct CleanupConfig {
    pub log_dir: PathBuf,
    pub log_file_pattern: String,
    pub retention_days: u64,
    pub cleanup_interval_seconds: u64,
    /// If set, delete statistics files (mtime) older than this many days (e.g. 30–90). None = never auto-delete statistics.
    pub statistics_retention_days: Option<u64>,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            log_dir: PathBuf::from("./logs"),
            log_file_pattern: "rct".to_string(),
            retention_days: 7,
            cleanup_interval_seconds: 3600, // 1 hour
            statistics_retention_days: Some(30),
        }
    }
}

/// Run background task to cleanup old logs on an interval.
/// Heavy I/O runs in spawn_blocking to avoid blocking the async runtime.
pub fn start_log_cleanup_task(config: CleanupConfig) {
    tokio::spawn(async move {
        let mut interval_timer =
            interval(time::Duration::from_secs(config.cleanup_interval_seconds));

        debug!(dir = ?config.log_dir, "[LogCleanup] task started");

        loop {
            interval_timer.tick().await;

            let config_clone = config.clone();
            match tokio::task::spawn_blocking(move || cleanup_old_logs(&config_clone)).await {
                Ok(Ok(deleted_count)) => {
                    if deleted_count > 0 {
                        info!(deleted_count, "[LogCleanup] cleaned up old log files");
                    }
                }
                Ok(Err(e)) => {
                    error!(error = %e, "[LogCleanup] failed to cleanup old log files");
                }
                Err(e) => {
                    error!(error = %e, "[LogCleanup] cleanup task join error");
                }
            }
        }
    });
}

/// Remove log dirs older than retention_days. Each dir is a date (YYYY-MM-DD); remove whole dir so grep only needs to search fewer dirs.
/// Also remove old flat files (legacy layout: files directly in log_dir) by mtime for backward compatibility.
/// **Statistics:** If `statistics_retention_days` is Some(n), delete files under `statistics/` with mtime older than n days. None = never auto-delete.
///
/// **Scale:** Iterates only direct children of log_dir (one entry per date dir + statistics + flat files). Statistics cleanup lists files in one dir and deletes by mtime; I/O runs in spawn_blocking so async runtime is not blocked.
fn cleanup_old_logs(
    config: &CleanupConfig,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let log_dir = &config.log_dir;

    if !log_dir.exists() {
        return Ok(0);
    }

    let cutoff_date = Local::now() - Duration::days(config.retention_days as i64);
    let cutoff_ymd = cutoff_date.format("%Y-%m-%d").to_string();
    let cutoff_timestamp = cutoff_date.timestamp_millis();
    let stats_cutoff_timestamp = config
        .statistics_retention_days
        .map(|d| (Local::now() - Duration::days(d as i64)).timestamp_millis());

    let mut deleted_count = 0;

    let entries = fs::read_dir(log_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if name == "statistics" {
                if let Some(cutoff_ts) = stats_cutoff_timestamp {
                    deleted_count += cleanup_statistics_dir(&path, cutoff_ts)?;
                }
                continue;
            }
            if name.len() != 10
                || name.chars().filter(|&c| c == '-').count() != 2
                || !name.chars().all(|c| c.is_ascii_digit() || c == '-')
            {
                continue;
            }
            if name >= cutoff_ymd.as_str() {
                continue;
            }
            match fs::remove_dir_all(&path) {
                Ok(_) => {
                    debug!(dir = ?path, "[LogCleanup] deleted old log directory");
                    deleted_count += 1;
                }
                Err(e) => {
                    warn!(dir = ?path, error = %e, "[LogCleanup] failed to delete old log directory");
                }
            }
            continue;
        }

        if !path.is_file() {
            continue;
        }

        // Flat files (legacy layout): delete by mtime
        let file_name_str = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        if !file_name_str.starts_with(&config.log_file_pattern) {
            continue;
        }
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                warn!(file = ?path, error = %e, "[LogCleanup] failed to get file metadata, skipping");
                continue;
            }
        };
        let modified_time = metadata.modified()?;
        let modified_timestamp = modified_time
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        if modified_timestamp < cutoff_timestamp {
            match fs::remove_file(&path) {
                Ok(_) => {
                    debug!(file = ?path, "[LogCleanup] deleted old flat log file");
                    deleted_count += 1;
                }
                Err(e) => {
                    warn!(file = ?path, error = %e, "[LogCleanup] failed to delete old log file");
                }
            }
        }
    }

    Ok(deleted_count)
}

/// Delete files under statistics_dir with mtime older than cutoff_timestamp (millis). Returns number of files deleted.
/// If statistics_dir does not exist or is not a directory, returns Ok(0) (no error).
fn cleanup_statistics_dir(
    statistics_dir: &std::path::Path,
    cutoff_timestamp: i64,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let mut deleted = 0;
    if !statistics_dir.is_dir() {
        return Ok(0);
    }
    let entries = match fs::read_dir(statistics_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let meta = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                warn!(file = ?path, error = %e, "[LogCleanup] statistics file metadata failed, skipping");
                continue;
            }
        };
        let modified = match meta.modified() {
            Ok(t) => t,
            Err(e) => {
                warn!(file = ?path, error = %e, "[LogCleanup] statistics file modified time failed, skipping");
                continue;
            }
        };
        let ts = modified
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        if ts < cutoff_timestamp {
            if fs::remove_file(&path).is_ok() {
                debug!(file = ?path, "[LogCleanup] deleted old statistics file");
                deleted += 1;
            } else {
                warn!(file = ?path, "[LogCleanup] failed to delete old statistics file");
            }
        }
    }
    Ok(deleted)
}

/// Run cleanup once immediately (do not wait for interval).
#[allow(dead_code)]
pub async fn cleanup_old_logs_async(
    config: &CleanupConfig,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let config_clone = config.clone();
    tokio::task::spawn_blocking(move || cleanup_old_logs(&config_clone))
        .await
        .map_err(|e| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Task join error: {}", e),
            )) as Box<dyn std::error::Error + Send + Sync>
        })?
}
