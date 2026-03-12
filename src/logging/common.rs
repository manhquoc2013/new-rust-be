//! Shared utilities and types for file logging and statistics.
//! - Node ID (instance identifier for log file names)
//! - Rotated filename timestamp (HH-MM-SS-ms), O(1) — no directory listing
//! - SanitizeWriter (plain-text safe for cat/tail/grep)
//! - Shared constants (flush interval, buffer size) to avoid duplication and drift

use once_cell::sync::Lazy;
use std::io::Write;

/// Flush interval (64KB): tail/grep see data soon; shared by main log and statistics writers.
pub(crate) const FLUSH_INTERVAL_BYTES: usize = 64 * 1024;
/// Default buffer size (256KB) for BufWriter; shared by main log and statistics.
pub(crate) const BUFFER_CAPACITY_256K: usize = 256 * 1024;

/// Node ID for log file names: from env `LOG_NODE_ID`, or a random 6-char hex string (created once per process).
/// Shared by file_logger and statistics (suffix: `{type}-{node_id}`).
pub fn get_or_create_node_id() -> String {
    static NODE_ID: Lazy<String> = Lazy::new(|| {
        std::env::var("LOG_NODE_ID")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                let u = uuid::Uuid::new_v4();
                u.simple().to_string().chars().take(6).collect::<String>()
            })
    });
    NODE_ID.clone()
}

/// Timestamp for rotated filename: HH-MM-SS-ms (e.g. 18-32-44-123). O(1), no directory listing; unique per rotation.
/// Shared by file_logger (daily_rotating_writer) and statistics.
pub(crate) fn rotated_timestamp() -> String {
    chrono::Local::now()
        .format("%H-%M-%S%.3f")
        .to_string()
        .replace('.', "-")
}

/// Writer that replaces null (0x00) and C0 control (0x01..=0x1F except \\n \\r \\t) with safe text,
/// so log/statistics files stay plain text for cat/tail/grep.
pub struct SanitizeWriter<W: Write> {
    inner: W,
}

impl<W: Write> SanitizeWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }

    #[inline]
    fn has_bad_bytes(buf: &[u8]) -> bool {
        buf.iter()
            .any(|&b| b == 0 || (b >= 0x01 && b <= 0x1F && b != 0x09 && b != 0x0A && b != 0x0D))
    }

    #[inline]
    fn sanitize_and_write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if !Self::has_bad_bytes(buf) {
            self.inner.write_all(buf)?;
            return Ok(buf.len());
        }
        const BACKSLASH_ZERO: &[u8] = b"\\0";
        const DOT: u8 = b'.';
        let mut i = 0;
        while i < buf.len() {
            let b = buf[i];
            let (replacement, advance) = if b == 0 {
                (BACKSLASH_ZERO, 1)
            } else if b >= 0x01 && b <= 0x1F && b != 0x09 && b != 0x0A && b != 0x0D {
                ([DOT].as_slice(), 1)
            } else {
                (buf[i..i + 1].as_ref(), 1)
            };
            self.inner.write_all(replacement)?;
            i += advance;
        }
        Ok(buf.len())
    }
}

impl<W: Write> Write for SanitizeWriter<W> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.sanitize_and_write(buf)?;
        Ok(buf.len())
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write> SanitizeWriter<W> {
    /// Access inner writer (e.g. to truncate file after rotate in same thread so no data is lost).
    pub fn get_inner_mut(&mut self) -> &mut W {
        &mut self.inner
    }
}
