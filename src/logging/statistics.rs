//! Statistics logging: DB retry attempts and DB operation timing.
//! Writes to logs/statistics/ with daily-rotating files, same pattern as file_logger (one writer per file, flush periodically).
//! File names include node_id (same as main logging) so multiple instances write to separate files.
//!
//! **DB retries:** Only retries triggered from the ETDR sync task (src/models/ETDR/sync/task.rs) that calls retry_save_etdr_to_db (retry.rs) are logged; inline retries from commit handlers are not.
//!
//! **Cleanup:** Optional auto-cleanup by `CleanupConfig::statistics_retention_days` (e.g. 30–90 days by mtime). None = manual only.
//!
//! **Same as file_logger:** One worker thread per file, BufWriter + flush every 64KB, SanitizeWriter so output is plain text (cat/tail/grep safe). lossy=false so no records are dropped when queue is full.
//! **Rotation:** By day (new file per day) and by size. Size rotation is done **only inside the writer thread**: after each flush we check file size; if >= max_file_size we sync, copy the full file to -N.txt, then truncate the same file (via the open handle). So no content is ever cut.
//! **No data loss:** Uses lock() so every line is queued to the NonBlocking writer (internal queue 64k lines, lossy=false). When the writer is busy the caller blocks briefly until the line is enqueued; the worker thread then writes to file.

use crate::logging::common::{
    get_or_create_node_id, rotated_timestamp, SanitizeWriter, BUFFER_CAPACITY_256K,
    FLUSH_INTERVAL_BYTES,
};
use once_cell::sync::Lazy;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Mutex;

/// Daily-rotating file writer for statistics: path = log_dir/statistics/{base_name}-{node_id}-yyyy-mm-dd.txt
/// Rotates by size inside the writer (after flush): copy full file to -N.txt then truncate same handle so no content is cut.
struct DailyRotatingStatisticsWriter {
    statistics_dir: PathBuf,
    base_name: String,
    node_id: String,
    max_file_size: u64,
    current_date: String,
    bytes_since_flush: usize,
    inner: Option<BufWriter<SanitizeWriter<std::fs::File>>>,
}

impl DailyRotatingStatisticsWriter {
    fn new(
        statistics_dir: PathBuf,
        base_name: String,
        node_id: String,
        max_file_size: u64,
    ) -> Self {
        Self {
            statistics_dir,
            base_name,
            node_id,
            max_file_size,
            current_date: String::new(),
            bytes_since_flush: 0,
            inner: None,
        }
    }

    fn path_for_date(&self, date: &str) -> PathBuf {
        self.statistics_dir
            .join(format!("{}-{}-{}.txt", self.base_name, self.node_id, date))
    }

    /// Rotate by size in the same thread: flush is already done, so copy full file to -HH-MM-SS-ms then truncate this handle. No content lost.
    /// Rotated name uses timestamp (no directory listing, O(1), scales to any number of files).
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
        let prefix = format!("{}-{}-{}-", self.base_name, self.node_id, self.current_date);
        let ts = rotated_timestamp();
        let rotated_name = format!("{}{}.txt", prefix, ts);
        let rotated_path = self.statistics_dir.join(&rotated_name);
        if let Some(ref mut w) = self.inner {
            w.get_mut().get_inner_mut().sync_all()?;
        }
        std::fs::copy(&path, &rotated_path)?;
        if let Some(ref mut w) = self.inner {
            w.get_mut().get_inner_mut().set_len(0)?;
        }
        self.bytes_since_flush = 0;
        Ok(())
    }

    fn ensure_file_for_today(&mut self) -> std::io::Result<()> {
        let need_date_check = self.inner.is_none()
            || self.current_date.is_empty()
            || self.bytes_since_flush >= FLUSH_INTERVAL_BYTES;
        if !need_date_check {
            return Ok(());
        }
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        if self.current_date == today && self.inner.is_some() {
            return Ok(());
        }
        if let Some(mut w) = self.inner.take() {
            let _ = w.flush();
        }
        std::fs::create_dir_all(&self.statistics_dir)?;
        let path = self.path_for_date(&today);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .write(true)
            .open(&path)?;
        self.current_date = today;
        self.bytes_since_flush = 0;
        self.inner = Some(BufWriter::with_capacity(
            BUFFER_CAPACITY_256K,
            SanitizeWriter::new(file),
        ));
        Ok(())
    }
}

unsafe impl Send for DailyRotatingStatisticsWriter {}

impl Write for DailyRotatingStatisticsWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.ensure_file_for_today()?;
        let n = if let Some(ref mut w) = self.inner {
            w.write(buf)?
        } else {
            0
        };
        self.bytes_since_flush += n;
        if self.bytes_since_flush >= FLUSH_INTERVAL_BYTES {
            if let Some(ref mut w) = self.inner {
                w.flush()?;
            }
            self.bytes_since_flush = 0;
            self.rotate_by_size_if_needed()?;
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(ref mut w) = self.inner {
            w.flush()
        } else {
            Ok(())
        }
    }
}

static RETRIES_WRITER: Lazy<Mutex<Option<Box<dyn Write + Send>>>> = Lazy::new(|| Mutex::new(None));
static TIME_WRITER: Lazy<Mutex<Option<Box<dyn Write + Send>>>> = Lazy::new(|| Mutex::new(None));
static STATS_GUARDS: Lazy<Mutex<Vec<tracing_appender::non_blocking::WorkerGuard>>> =
    Lazy::new(|| Mutex::new(Vec::new()));

/// Initialize statistics logging: create logs/statistics and wire writers.
/// Uses same node_id as main logging (from LOG_NODE_ID or generated at startup).
/// Spawns background task to rotate statistics files by size (same max_file_size as main logs).
/// Call from init_logging after creating log_dir.
pub fn init_statistics(max_file_size: u64) {
    let statistics_dir = PathBuf::from("statistics");
    let _ = std::fs::create_dir_all(&statistics_dir);
    let node_id = get_or_create_node_id();

    let retries_writer = DailyRotatingStatisticsWriter::new(
        statistics_dir.to_path_buf(),
        "rct-db-retries".to_string(),
        node_id.clone(),
        max_file_size,
    );
    let (retries_nb, retries_guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .buffered_lines_limit(64_000)
        .lossy(false)
        .thread_name("log-statistics-retries")
        .finish(retries_writer);

    let time_writer = DailyRotatingStatisticsWriter::new(
        statistics_dir.to_path_buf(),
        "rct-db-time".to_string(),
        node_id.clone(),
        max_file_size,
    );
    let (time_nb, time_guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .buffered_lines_limit(64_000)
        .lossy(false)
        .thread_name("log-statistics-time")
        .finish(time_writer);

    {
        let mut guards = STATS_GUARDS.lock().unwrap();
        guards.clear();
        guards.push(retries_guard);
        guards.push(time_guard);
    }
    {
        let mut w = RETRIES_WRITER.lock().unwrap();
        *w = Some(Box::new(retries_nb));
    }
    {
        let mut w = TIME_WRITER.lock().unwrap();
        *w = Some(Box::new(time_nb));
    }
}

/// Log one DB retry attempt: <timestamp>   <raw_content>   <success|error>
/// Raw content is normalized to one line (newlines replaced). Blocks until line is enqueued (no data loss); writer has internal queue 64k lines.
#[inline]
pub fn log_db_retry(raw_content: &str, success: bool) {
    let result_str = if success { "success" } else { "error" };
    let timestamp = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f");
    let line = if raw_content.contains('\n') || raw_content.contains('\r') {
        format!(
            "{}   {}   {}\n",
            timestamp,
            raw_content.replace('\n', " ").replace('\r', " "),
            result_str
        )
    } else {
        format!("{}   {}   {}\n", timestamp, raw_content, result_str)
    };
    if let Ok(mut opt) = RETRIES_WRITER.lock() {
        if let Some(ref mut w) = *opt {
            let _ = w.write_all(line.as_bytes());
        }
    }
}

/// Log one DB operation timing for easier tracking.
///
/// Format: `<timestamp>   <OP>   <table>   <detail>   <elapsed_ms>`
/// - **table**: Entity/table name (e.g. TRANSPORT_TRANSACTION_STAGE, BOO_TRANSPORT_TRANS_STAGE); use "-" when omitted.
/// - **detail**: Id (e.g. "12345"), query type (e.g. "by_etag", "by_ticket_in_id"), or "-" when omitted.
/// Blocks until line is enqueued (no data loss); writer has internal queue 64k lines.
#[inline]
pub fn log_db_time(operation: &str, elapsed_ms: u64, table: Option<&str>, detail: Option<&str>) {
    let timestamp = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f");
    let op_upper = match operation {
        "get" | "GET" => "GET",
        "insert" | "INSERT" => "INSERT",
        "update" | "UPDATE" => "UPDATE",
        "delete" | "DELETE" => "DELETE",
        other => other,
    };
    let table_str = table.unwrap_or("-");
    let detail_str = detail.unwrap_or("-");
    let line = format!(
        "{}   {}   {}   {}   {}\n",
        timestamp, op_upper, table_str, detail_str, elapsed_ms
    );
    if let Ok(mut opt) = TIME_WRITER.lock() {
        if let Some(ref mut w) = *opt {
            let _ = w.write_all(line.as_bytes());
        }
    }
}
