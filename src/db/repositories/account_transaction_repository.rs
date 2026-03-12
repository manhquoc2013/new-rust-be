//! Ghi nhận giao dịch tài khoản (ACCOUNT_TRANSACTION) cho BECT: tạo bản ghi lúc checkout, cập nhật trạng thái lúc commit.
//!
//! Theo Java (docs/mediation-server-master): ChargeType.AUTOCOMMIT("1"), PENDING_COMMIT("2");
//! AutocommitStatus: COMMIT("1"), ROLLBACK("2"); null = đang reserve.
//!
//! - **AUTOCOMMIT** (gắn khi checkout): 1 = autocommit (tự hoàn tất ngay, không cần commit sau);
//!   2 = pending commit (chờ commit). Đây là cơ sở để commit có tự động hoàn tất và cập nhật hay không.
//! - **AUTOCOMMIT_STATUS**: khi checkout (reserve) để **null**; khi **commit** mới gắn 1 (COMMIT) hoặc 2 (ROLLBACK).
//!   allowCommitOrRollback() = (AUTOCOMMIT_STATUS == null && AUTOCOMMIT != 1) → chỉ cập nhật khi AUTOCOMMIT=2 và AUTOCOMMIT_STATUS null.
//!
//! ## Ánh xạ trường với Java (reserve/commit)
//!
//! **Insert (checkout – reserve):** Theo `AccountTransaction.reserve()` và `AccountDBService.reserve()`:
//! | Trường Rust/DB        | Nguồn / Logic |
//! |-----------------------|----------------|
//! | ACCOUNT_TRANSACTION_ID| Sequence ACCOUNT_TRANSACTION_SEQ |
//! | ACCOUNT_ID            | account_id từ subscriber |
//! | OLD_BALANCE           | balance tài khoản trước checkout (ACCOUNT.BALANCE) |
//! | NEW_BALANCE           | old_balance - amount (Java: newBalance = oldBalance - amount) |
//! | AMOUNT                | trans_amount (phí tính tại checkout) |
//! | DESCRIPTION           | Mô tả, ví dụ "ETC checkout transport_trans_id=..." |
//! | TRANS_DATETIME        | Thời điểm checkout |
//! | AUTOCOMMIT            | 2 (PENDING_COMMIT – chờ commit) |
//! | AUTOCOMMIT_STATUS     | Không set (null) |
//! | ACCOUNT_TRANS_TYPE    | 2 (CHARGE) |
//! | INSUFF_AMOUNT         | 0 nếu đủ tiền; (amount - old_available_bal) nếu thiếu |
//! | OLD_AVAILABLE_BAL     | AVAILABLE_BALANCE trước checkout |
//! | NEW_AVAILABLE_BAL     | old_available_bal - amount (lock) |
//! | CREATED_DATE          | trans_datetime |
//! | TRANS_TYPE            | Loại nghiệp vụ, ví dụ "ETC_CHECKOUT" (Java: null→"NAN") |
//! | CHARGE_IN             | "E" (ETC/BECT) |
//!
//! **Update (checkout commit):** Chỉ khi allowCommitOrRollback (AUTOCOMMIT=2, AUTOCOMMIT_STATUS null).
//! | Trường                | Success: COMMIT(1) | Fail: ROLLBACK(2) |
//! |-----------------------|--------------------|-------------------|
//! | AUTOCOMMIT_STATUS     | 1                  | 2                 |
//! | FINISH_DATETIME       | now                | now               |
//! | NEW_BALANCE           | old_balance - amount | old_balance (giữ) |
//! | NEW_AVAILABLE_BAL     | old_available_bal - amount | old_available_bal (khôi phục) |
//! | OLD_AVAILABLE_BAL_ROL | (không set)        | old_available_bal |
//! | NEW_AVAILABLE_BAL_ROL | (không set)        | old_available_bal |

use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::constants::account_transaction;
use crate::db::repository::format_sql_datetime_str;
use crate::db::sequence::get_next_sequence_value_with_schema;
use crate::db::{get_nullable_i32, DbError};
use odbc_api::Cursor;
use r2d2::Pool;

const TABLE_NAME: &str = "CRM_OWNER.ACCOUNT_TRANSACTION";
/// Sequence cho ACCOUNT_TRANSACTION_ID (CRM_OWNER.ACCOUNT_TRANSACTION_SEQ, CACHE 20 NOCYCLE).
const SEQ_SCHEMA: &str = "CRM_OWNER";
const SEQ_NAME: &str = "ACCOUNT_TRANSACTION_SEQ";

/// Tham số insert bản ghi ACCOUNT_TRANSACTION lúc BECT checkout.
pub struct InsertAccountTransactionParams {
    pub account_id: i64,
    pub old_balance: f64,
    pub new_balance: f64,
    pub amount: f64,
    pub description: String,
    pub trans_datetime: String,
    pub old_available_bal: f64,
    pub new_available_bal: f64,
    pub insuff_amount: f64,
    pub trans_type: Option<String>,
    pub transport_trans_id: i64,
}

/// Insert bản ghi ACCOUNT_TRANSACTION khi BECT checkout (reserve).
/// AUTOCOMMIT=2 (PENDING_COMMIT), AUTOCOMMIT_STATUS không set (null) – cơ sở để commit sau đó hoàn tất và cập nhật.
/// Trả về ACCOUNT_TRANSACTION_ID để lưu vào TRANSPORT_TRANSACTION_STAGE.ACCOUNT_TRANS_ID.
pub fn insert_account_transaction_for_bect_checkout(
    pool: &Pool<OdbcConnectionManager>,
    params: &InsertAccountTransactionParams,
) -> Result<i64, DbError> {
    let id = get_next_sequence_value_with_schema(pool, SEQ_SCHEMA, SEQ_NAME)?;
    let conn = get_connection_with_retry(pool).map_err(|e| DbError::PoolError(e.to_string()))?;

    let desc_escaped = params.description.replace("'", "''");
    let trans_type_val = params
        .trans_type
        .as_deref()
        .unwrap_or("ETC_CHECKOUT")
        .replace("'", "''");
    let now = params.trans_datetime.as_str();

    let sql = format!(
        r#"INSERT INTO {} (ACCOUNT_TRANSACTION_ID, ACCOUNT_ID, OLD_BALANCE, NEW_BALANCE, AMOUNT, DESCRIPTION,
           TRANS_DATETIME, AUTOCOMMIT, ACCOUNT_TRANS_TYPE, INSUFF_AMOUNT, OLD_AVAILABLE_BAL, NEW_AVAILABLE_BAL,
           CREATED_DATE, TRANS_TYPE, CHARGE_IN)
           VALUES ({}, {}, {}, {}, {}, '{}', {}, {}, {}, {}, {}, {}, {}, '{}', '{}')"#,
        TABLE_NAME,
        id,
        params.account_id,
        params.old_balance,
        params.new_balance,
        params.amount,
        desc_escaped,
        format_sql_datetime_str(now),
        account_transaction::AUTOCOMMIT_LOCK,
        account_transaction::ACCOUNT_TRANS_TYPE_CHARGE,
        params.insuff_amount,
        params.old_available_bal,
        params.new_available_bal,
        format_sql_datetime_str(now),
        trans_type_val,
        account_transaction::CHARGE_IN_ETC,
    );

    conn.execute(&sql, (), None)
        .map_err(|e| DbError::ExecutionError(e.to_string()))?;
    Ok(id)
}

/// Cập nhật trạng thái commit/rollback cho bản ghi ACCOUNT_TRANSACTION (BECT checkout commit).
pub fn update_account_transaction_commit_status(
    pool: &Pool<OdbcConnectionManager>,
    account_transaction_id: i64,
    autocommit_status: i32,
    finish_datetime: &str,
    new_balance: f64,
    new_available_bal: f64,
    old_available_bal_rol: Option<f64>,
    new_available_bal_rol: Option<f64>,
) -> Result<bool, DbError> {
    if account_transaction_id <= 0 {
        return Ok(false);
    }
    let conn = get_connection_with_retry(pool).map_err(|e| DbError::PoolError(e.to_string()))?;

    let mut set_parts = vec![
        format!("AUTOCOMMIT_STATUS = {}", autocommit_status),
        format!(
            "FINISH_DATETIME = {}",
            format_sql_datetime_str(finish_datetime)
        ),
        format!("NEW_BALANCE = {}", new_balance),
        format!("NEW_AVAILABLE_BAL = {}", new_available_bal),
    ];
    if let Some(v) = old_available_bal_rol {
        set_parts.push(format!("OLD_AVAILABLE_BAL_ROL = {}", v));
    }
    if let Some(v) = new_available_bal_rol {
        set_parts.push(format!("NEW_AVAILABLE_BAL_ROL = {}", v));
    }

    let sql = format!(
        "UPDATE {} SET {} WHERE ACCOUNT_TRANSACTION_ID = {}",
        TABLE_NAME,
        set_parts.join(", "),
        account_transaction_id
    );

    conn.execute(&sql, (), None)
        .map_err(|e| DbError::ExecutionError(e.to_string()))?;
    Ok(true)
}

/// Dữ liệu cần cho cập nhật commit (lấy từ bản ghi đã insert lúc checkout).
#[derive(Debug, Clone)]
pub struct AccountTransactionForCommit {
    pub old_balance: f64,
    pub old_available_bal: f64,
    pub amount: f64,
    /// AUTOCOMMIT tại checkout: 1 = autocommit (không cần cập nhật), 2 = pending (cho phép commit/rollback).
    pub autocommit: Option<i32>,
    /// AUTOCOMMIT_STATUS: null/empty = chưa commit/rollback; 1/2 = đã xử lý. Chỉ cập nhật khi null.
    pub autocommit_status: Option<i32>,
}

/// Trả về true nếu bản ghi được phép commit/rollback (theo Java allowCommitOrRollback).
#[inline]
pub fn allow_commit_or_rollback(autocommit: Option<i32>, autocommit_status: Option<i32>) -> bool {
    autocommit_status.is_none() && autocommit != Some(account_transaction::AUTOCOMMIT_AUTO)
}

/// Get record ACCOUNT_TRANSACTION theo ID (OLD_*, AMOUNT, AUTOCOMMIT, AUTOCOMMIT_STATUS cho commit).
pub fn get_account_transaction_for_commit(
    pool: &Pool<OdbcConnectionManager>,
    account_transaction_id: i64,
) -> Result<Option<AccountTransactionForCommit>, DbError> {
    if account_transaction_id <= 0 {
        return Ok(None);
    }
    let conn = get_connection_with_retry(pool).map_err(|e| DbError::PoolError(e.to_string()))?;
    let sql = format!(
        r#"SELECT OLD_BALANCE, OLD_AVAILABLE_BAL, AMOUNT, AUTOCOMMIT, AUTOCOMMIT_STATUS FROM {} WHERE ACCOUNT_TRANSACTION_ID = {}"#,
        TABLE_NAME, account_transaction_id
    );
    let cursor_opt = conn
        .execute(&sql, (), None)
        .map_err(|e| DbError::ExecutionError(e.to_string()))?;
    match cursor_opt {
        Some(mut cursor) => {
            let row = cursor
                .next_row()
                .map_err(|e| DbError::ExecutionError(e.to_string()))?;
            if let Some(mut r) = row {
                let mut old_balance: f64 = 0.0;
                let mut old_available_bal: f64 = 0.0;
                let mut amount: f64 = 0.0;
                r.get_data(1, &mut old_balance)
                    .map_err(|e| DbError::ConversionError(format!("OLD_BALANCE: {}", e)))?;
                r.get_data(2, &mut old_available_bal)
                    .map_err(|e| DbError::ConversionError(format!("OLD_AVAILABLE_BAL: {}", e)))?;
                r.get_data(3, &mut amount)
                    .map_err(|e| DbError::ConversionError(format!("AMOUNT: {}", e)))?;
                let autocommit = get_nullable_i32(&mut r, 4)?;
                let autocommit_status = get_nullable_i32(&mut r, 5)?;
                Ok(Some(AccountTransactionForCommit {
                    old_balance,
                    old_available_bal,
                    amount,
                    autocommit,
                    autocommit_status,
                }))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}
