//! DB operation error type (pool, execution, not found, conversion, ...).

use std::fmt;

/// Lỗi thao tác database.
#[derive(Debug)]
pub enum DbError {
    /// Lỗi pool kết nối
    PoolError(String),
    /// Lỗi thực thi SQL
    ExecutionError(String),
    /// Không có row (mong đợi có một)
    NotFound(String),
    /// Lỗi chuyển đổi/parse dữ liệu
    ConversionError(String),
    /// Dữ liệu đầu vào không hợp lệ
    #[allow(dead_code)]
    InvalidInput(String),
    /// Lỗi khác
    Other(String),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::PoolError(msg) => write!(f, "Pool error: {}", msg),
            DbError::ExecutionError(msg) => write!(f, "Execution error: {}", msg),
            DbError::NotFound(msg) => write!(f, "Not found: {}", msg),
            DbError::ConversionError(msg) => write!(f, "Conversion error: {}", msg),
            DbError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            DbError::Other(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl std::error::Error for DbError {}

impl From<odbc_api::Error> for DbError {
    fn from(err: odbc_api::Error) -> Self {
        // Capture full ODBC message; Display can truncate, Debug often has full driver text.
        let display_msg = err.to_string();
        let debug_msg = format!("{:?}", err);
        let error_msg = if debug_msg.len() > display_msg.len() && !debug_msg.is_empty() {
            format!("{} (detail: {})", display_msg, debug_msg)
        } else {
            display_msg
        };
        DbError::ExecutionError(error_msg)
    }
}

impl From<r2d2::Error> for DbError {
    fn from(err: r2d2::Error) -> Self {
        DbError::PoolError(err.to_string())
    }
}
