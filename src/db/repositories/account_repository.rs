//! Tra cứu trạng thái và số dư tài khoản (ACCOUNT) cho BECT.
//!
//! Tất cả các bảng dưới đây thuộc schema **CRM_OWNER**.
//!
//! ## Bảng CRM_OWNER.ACCOUNT
//! | Column              | Comment |
//! |---------------------|--------|
//! | ACCOUNT_ID          | Khóa chính, định danh duy nhất của tài khoản |
//! | ACCOUNT_NUMBER      | Số tài khoản giao thông |
//! | BALANCE             | Số dư hiện tại của tài khoản |
//! | AVAILABLE_BALANCE   | Số dư khả dụng có thể sử dụng |
//! | LOCK_BALANCE        | Số tiền bị khóa, không thể sử dụng |
//! | LAST_MODIFY         | Thời điểm cập nhật thông tin tài khoản gần nhất |
//! | STATUS              | 1: hoạt động, 0: hủy, 2: tạm khóa |
//! | CUST_TYPE           | I: cá nhân, C: doanh nghiệp, G: cơ quan nhà nước |
//! | SUBSCRIPTION_BALANCE| Số dư liên quan đến gói dịch vụ/thuê bao |
//! | MIG_OBJECT_TYPE, MIG_OBJECT_ID, SOURCE, PARENT_ACCOUNT_ID, ... |
//!
//! ## Bảng CRM_OWNER.SUBSCRIBER (liên kết với ACCOUNT)
//! | Column        | Comment |
//! |---------------|--------|
//! | SUBSCRIBER_ID | Khóa chính |
//! | ACCOUNT_ID    | ID tài khoản giao thông liên kết |
//! | ETAG_ID, VEHICLE_ID | Liên kết eTag – phương tiện |
//! | STATUS        | 0: không hoạt động, 1: đang hoạt động, 2: đã bán xe |
//! | START_DATE, VEHICLE_TYPE, VEHICLE_OWNER, ... |
//!
//! ## Bảng CRM_OWNER.ACCOUNT_TRANSACTION (ghi giao dịch trừ tiền)
//! | Column               | Comment |
//! |-----------------------|--------|
//! | ACCOUNT_TRANSACTION_ID| Khóa chính |
//! | ACCOUNT_ID            | Tài khoản liên quan |
//! | OLD_BALANCE, NEW_BALANCE, AMOUNT | Số dư trước/sau, số tiền giao dịch |
//! | OLD_AVAILABLE_BAL, NEW_AVAILABLE_BAL | Số dư khả dụng trước/sau |
//! | ACCOUNT_TRANS_TYPE    | 1: recharge, 2: charge, 3: adjustment |
//! | TRANS_TYPE, CHARGE_IN | Loại chi tiết; nguồn trừ (W/E) |
//!
//! Nếu bảng CRM_OWNER.ACCOUNT không tồn tại hoặc thiếu cột, query sẽ lỗi; caller nên bắt lỗi nếu cần.

use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::DbError;
use odbc_api::{Cursor, IntoParameter};
use r2d2::Pool;

/// Kết quả tra cứu tài khoản cho giao dịch trừ phí (BECT).
#[derive(Debug, Clone)]
pub struct AccountForCharge {
    /// STATUS: 1 = hoạt động, 0 = hủy, 2 = tạm khóa (lưu dạng string để tương thích CHAR/NUMBER).
    pub status: String,
    /// Số dư khả dụng (AVAILABLE_BALANCE) – đơn vị VNĐ, có thể lưu NUMBER(20,2) trong DB.
    pub available_balance: f64,
    /// Số dư tài khoản (BALANCE) – dùng cho ghi ACCOUNT_TRANSACTION OLD_BALANCE.
    pub balance: f64,
}

/// Trạng thái tài khoản theo bảng ACCOUNT.
pub mod account_status {
    /// Hoạt động – được phép trừ phí.
    pub const ACTIVE: &str = "1";
    /// Đã hủy.
    pub const CANCELLED: &str = "0";
    /// Tạm khóa.
    pub const LOCKED: &str = "2";
}

/// Lấy trạng thái và số dư khả dụng theo ACCOUNT_ID.
/// Trả về None nếu không tìm thấy hoặc query lỗi (bảng không tồn tại, etc.).
pub fn get_account_for_charge(
    pool: &Pool<OdbcConnectionManager>,
    account_id: i64,
) -> Result<Option<AccountForCharge>, DbError> {
    if account_id <= 0 {
        return Ok(None);
    }
    let conn = get_connection_with_retry(pool).map_err(|e| DbError::PoolError(e.to_string()))?;

    // ACCOUNT: STATUS, AVAILABLE_BALANCE, BALANCE (cho ghi ACCOUNT_TRANSACTION).
    let sql = r#"
        SELECT STATUS, NVL(AVAILABLE_BALANCE,0), NVL(BALANCE,0)
        FROM CRM_OWNER.ACCOUNT
        WHERE ACCOUNT_ID = ?
    "#;

    let param = account_id.into_parameter();
    let params = (&param,);

    let cursor_opt = conn
        .execute(sql, params, None)
        .map_err(|e| DbError::ExecutionError(e.to_string()))?;

    match cursor_opt {
        Some(mut cursor) => {
            let row = cursor
                .next_row()
                .map_err(|e| DbError::ExecutionError(e.to_string()))?;

            if let Some(mut r) = row {
                // STATUS: có thể NUMBER(1) hoặc VARCHAR2; đọc dạng text (driver thường convert NUMBER sang "1"/"0"/"2").
                let mut status_buf = Vec::new();
                r.get_text(1, &mut status_buf)
                    .map_err(|e| DbError::ConversionError(format!("STATUS: {}", e)))?;
                let status = String::from_utf8_lossy(&status_buf)
                    .trim_end_matches('\0')
                    .trim()
                    .to_string();

                // AVAILABLE_BALANCE: thường NUMBER(20,2) – đọc dạng f64.
                let mut available_balance: f64 = 0.0;
                r.get_data(2, &mut available_balance)
                    .map_err(|e| DbError::ConversionError(format!("AVAILABLE_BALANCE: {}", e)))?;

                let mut balance: f64 = 0.0;
                r.get_data(3, &mut balance)
                    .map_err(|e| DbError::ConversionError(format!("BALANCE: {}", e)))?;

                Ok(Some(AccountForCharge {
                    status,
                    available_balance,
                    balance,
                }))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}
