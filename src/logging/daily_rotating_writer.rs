//! Daily + size rotating file writer for log files (one file per process type and level).
//! Writes to log_dir/YYYY-MM-DD/{name}-{suffix}.log; rotates by day and by size (in writer thread: sync → copy → truncate, no data loss).
//! **No limit on number of rotated files** per day; cleanup is by retention_days (delete old date dirs). Flush every 64KB so cat/tail/grep see data.

use crate::logging::common::{
    rotated_timestamp, SanitizeWriter, BUFFER_CAPACITY_256K, FLUSH_INTERVAL_BYTES,
};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

/// BufWriter buffer size for main log (256KB from common).
pub(crate) const DEFAULT_LOG_BUFFER_CAPACITY: usize = BUFFER_CAPACITY_256K;
/// Buffer size for debug file (512KB).
pub(crate) const DEBUG_LOG_BUFFER_CAPACITY: usize = 512 * 1024;
/// Max lines in non-blocking queue per process-type file.
pub(crate) const MAIN_NON_BLOCKING_QUEUE_LINES: usize = 256_000;
/// Max lines in queue for debug file.
pub(crate) const DEBUG_NON_BLOCKING_QUEUE_LINES: usize = 512_000;

/// Writer for log file; opens a new file when the date changes (daily rotation).
/// Size rotation: only in writer thread after flush — sync, copy full file to -HH-MM-SS-ms.log, truncate same handle. No cap on rotated file count.
/// **Re-open on error:** If write or flush returns an IO error, the current file handle is closed; the next write will re-open the file (best-effort recovery).
pub(crate) struct DailyRotatingFileWriter {
    log_dir: PathBuf,
    log_file_name: String,
    suffix: String,
    max_file_size: u64,
    current_date: String,
    buffer_capacity: usize,
    bytes_since_flush: usize,
    inner: Option<BufWriter<SanitizeWriter<std::fs::File>>>,
}

impl DailyRotatingFileWriter {
    pub(crate) fn new(
        log_dir: PathBuf,
        log_file_name: String,
        suffix: String,
        max_file_size: u64,
    ) -> Self {
        Self::with_buffer_capacity(
            log_dir,
            log_file_name,
            suffix,
            max_file_size,
            DEFAULT_LOG_BUFFER_CAPACITY,
        )
    }

    pub(crate) fn with_buffer_capacity(
        log_dir: PathBuf,
        log_file_name: String,
        suffix: String,
        max_file_size: u64,
        buffer_capacity: usize,
    ) -> Self {
        Self {
            log_dir,
            log_file_name,
            suffix,
            max_file_size,
            current_date: String::new(),
            buffer_capacity,
            bytes_since_flush: 0,
            inner: None,
        }
    }

    fn path_for_date(&self, date: &str) -> PathBuf {
        self.log_dir
            .join(date)
            .join(format!("{}-{}.log", self.log_file_name, self.suffix))
    }

    fn rotate_by_size_if_needed(&mut self) -> std::io::Result<()> {
        if self.max_file_size == 0 || self.current_date.is_empty() {
            return Ok(());
        }
        let path = self.path_for_date(&self.current_date);
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => return Ok(()),
        };
        if meta.len() < self.max_file_size {
            return Ok(());
        }
        let date_dir = self.log_dir.join(&self.current_date);
        let prefix = format!("{}-{}-", self.log_file_name, self.suffix);
        let ts = rotated_timestamp();
        let rotated_name = format!("{}{}.log", prefix, ts);
        let rotated_path = date_dir.join(&rotated_name);
        if let Some(ref mut w) = self.inner {
            w.get_mut().get_inner_mut().sync_all()?;
        }
        if let Err(e) = std::fs::copy(&path, &rotated_path) {
            let _ = eprintln!(
                "[LogRotation] failed to copy for size rotation: {} -> {}: {}",
                path.display(),
                rotated_path.display(),
                e
            );
            return Err(e);
        }
        if let Some(ref mut w) = self.inner {
            w.get_mut().get_inner_mut().set_len(0)?;
        }
        self.bytes_since_flush = 0;
        Ok(())
    }

    fn ensure_file_for_today(&mut self) -> std::io::Result<()> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let need_date_check = self.inner.is_none()
            || self.current_date.is_empty()
            || self.current_date != today
            || self.bytes_since_flush >= FLUSH_INTERVAL_BYTES;
        if !need_date_check {
            return Ok(());
        }
        if self.current_date == today && self.inner.is_some() {
            return Ok(());
        }
        if let Some(mut w) = self.inner.take() {
            let _ = w.flush();
        }
        let date_dir = self.log_dir.join(&today);
        std::fs::create_dir_all(&date_dir)?;
        let path = self.path_for_date(&today);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .write(true)
            .open(&path)?;
        self.current_date = today;
        self.bytes_since_flush = 0;
        self.inner = Some(BufWriter::with_capacity(
            self.buffer_capacity,
            SanitizeWriter::new(file),
        ));
        Ok(())
    }

    /// Close the current file handle so the next write will re-open. Use after IO errors to allow recovery.
    fn close_inner_on_error(&mut self, context: &str, err: &std::io::Error) {
        if self.inner.take().is_some() {
            let _ = eprintln!(
                "[LogWriter] {}: {}; will re-open on next write (suffix={})",
                context, err, self.suffix
            );
        }
        self.bytes_since_flush = 0;
    }
}

impl Drop for DailyRotatingFileWriter {
    fn drop(&mut self) {
        if let Some(ref mut w) = self.inner {
            let _ = w.flush();
        }
    }
}

impl Write for DailyRotatingFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Err(e) = self.ensure_file_for_today() {
            self.close_inner_on_error("ensure_file_for_today", &e);
            return Err(e);
        }
        let n = match self.inner.as_mut() {
            Some(w) => match w.write(buf) {
                Ok(n) => n,
                Err(e) => {
                    self.close_inner_on_error("write", &e);
                    return Err(e);
                }
            },
            None => 0,
        };
        self.bytes_since_flush += n;
        if self.bytes_since_flush >= FLUSH_INTERVAL_BYTES {
            if let Some(ref mut w) = self.inner {
                if let Err(e) = w.flush() {
                    self.close_inner_on_error("flush", &e);
                    return Err(e);
                }
            }
            self.bytes_since_flush = 0;
            if let Err(e) = self.rotate_by_size_if_needed() {
                self.close_inner_on_error("rotate_by_size", &e);
                let _ = e;
            }
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(ref mut w) = self.inner {
            w.flush().map_err(|e| {
                self.close_inner_on_error("flush", &e);
                e
            })
        } else {
            Ok(())
        }
    }
}
