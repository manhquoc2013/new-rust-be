//! Lấy NEXTVAL/CURRVAL từ sequence Altibase (có/không schema).

use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::error::DbError;
use odbc_api::{Cursor, Nullable};
use r2d2::Pool;

/// Lấy giá trị tiếp theo từ sequence (NEXTVAL, Altibase).
pub fn get_next_sequence_value(
    pool: &Pool<OdbcConnectionManager>,
    sequence_name: &str,
) -> Result<i64, DbError> {
    let conn = get_connection_with_retry(pool)?;

    // Alias (AS NEXTVAL) tránh lỗi "Column not found" với một số driver ODBC/Oracle.
    let query = format!("SELECT {}.NEXTVAL AS NEXTVAL FROM DUAL", sequence_name);

    let cursor_result = conn.execute(&query, (), None)?;
    match cursor_result {
        Some(mut cursor) => match cursor.next_row() {
            Ok(Some(mut row)) => {
                let mut field = Nullable::<i64>::null();
                row.get_data(1, &mut field).map_err(|e| {
                    DbError::ConversionError(format!("Failed to get sequence value: {}", e))
                })?;
                field.into_opt().ok_or_else(|| {
                    DbError::ConversionError(format!("Sequence {} returned null", sequence_name))
                })
            }
            Ok(None) => Err(DbError::NotFound(format!(
                "No result from sequence {}",
                sequence_name
            ))),
            Err(e) => Err(DbError::ExecutionError(format!(
                "Error fetching sequence {}: {}",
                sequence_name, e
            ))),
        },
        None => Err(DbError::ExecutionError(format!(
            "Sequence query {} returned no cursor",
            sequence_name
        ))),
    }
}

/// Lấy giá trị hiện tại (CURRVAL); nếu chưa dùng trong session thì thử NEXTVAL (sequence tăng 1).
pub fn get_current_sequence_value(
    pool: &Pool<OdbcConnectionManager>,
    sequence_name: &str,
) -> Result<i64, DbError> {
    // Thử dùng CURRVAL trước
    {
        let conn = get_connection_with_retry(pool)?;
        let query_currval = format!("SELECT {}.CURRVAL AS CURRVAL FROM DUAL", sequence_name);
        let cursor_result = conn.execute(&query_currval, (), None)?;

        // Xử lý kết quả CURRVAL
        if let Some(mut cursor) = cursor_result {
            match cursor.next_row() {
                Ok(Some(mut row)) => {
                    let mut field = Nullable::<i64>::null();
                    row.get_data(1, &mut field).map_err(|e| {
                        DbError::ConversionError(format!(
                            "Failed to get current sequence value: {}",
                            e
                        ))
                    })?;
                    if let Some(value) = field.into_opt() {
                        return Ok(value);
                    } else {
                        return Err(DbError::ConversionError(format!(
                            "Sequence {} returned null",
                            sequence_name
                        )));
                    }
                }
                Ok(None) => {
                    // No row returned, sẽ thử NEXTVAL
                }
                Err(e) => {
                    // Kiểm tra xem có phải lỗi "sequence not defined in session" không
                    let error_msg = e.to_string();
                    if !error_msg.contains("not defined in this session")
                        && !error_msg.contains("200819")
                    {
                        // Lỗi khác, trả về lỗi
                        return Err(DbError::ExecutionError(format!(
                            "Error fetching sequence {}: {}",
                            sequence_name, e
                        )));
                    }
                    // Nếu là lỗi "not defined in session", sẽ thử NEXTVAL
                }
            }
        }
        // Nếu đến đây, CURRVAL không thành công, sẽ thử NEXTVAL
    } // conn được drop ở đây

    // Nếu CURRVAL thất bại vì sequence chưa được định nghĩa trong session,
    // thử dùng NEXTVAL (sẽ tăng sequence) và trả về giá trị đó - 1
    // Lưu ý: Điều này sẽ tăng sequence lên 1
    let conn2 = get_connection_with_retry(pool)?;
    let query_nextval = format!("SELECT {}.NEXTVAL AS NEXTVAL FROM DUAL", sequence_name);
    let cursor_result2 = conn2.execute(&query_nextval, (), None)?;

    match cursor_result2 {
        Some(mut cursor2) => {
            match cursor2.next_row() {
                Ok(Some(mut row2)) => {
                    let mut field2 = Nullable::<i64>::null();
                    row2.get_data(1, &mut field2).map_err(|e| {
                        DbError::ConversionError(format!("Failed to get nextval: {}", e))
                    })?;
                    if let Some(nextval) = field2.into_opt() {
                        // Trả về giá trị NEXTVAL - 1 vì đã bị tăng khi gọi NEXTVAL
                        // Đây là giá trị "hiện tại" trước khi tăng
                        Ok(nextval - 1)
                    } else {
                        Err(DbError::ConversionError(format!(
                            "Sequence {} NEXTVAL returned null",
                            sequence_name
                        )))
                    }
                }
                Ok(None) => Err(DbError::NotFound(format!(
                    "No result from sequence {} NEXTVAL",
                    sequence_name
                ))),
                Err(e2) => Err(DbError::ExecutionError(format!(
                    "Error fetching sequence {} NEXTVAL: {}",
                    sequence_name, e2
                ))),
            }
        }
        None => Err(DbError::ExecutionError(format!(
            "Sequence NEXTVAL query {} returned no cursor",
            sequence_name
        ))),
    }
}

/// Lấy NEXTVAL với tên đủ schema (schema.sequence_name).
pub fn get_next_sequence_value_with_schema(
    pool: &Pool<OdbcConnectionManager>,
    schema: &str,
    sequence_name: &str,
) -> Result<i64, DbError> {
    let full_sequence_name = format!("{}.{}", schema, sequence_name);
    get_next_sequence_value(pool, &full_sequence_name)
}

/// Lấy CURRVAL với schema; nếu chưa có trong session thì thử system table hoặc NEXTVAL.
pub fn get_current_sequence_value_with_schema(
    pool: &Pool<OdbcConnectionManager>,
    schema: &str,
    sequence_name: &str,
) -> Result<i64, DbError> {
    let full_sequence_name = format!("{}.{}", schema, sequence_name);

    // Thử query từ system table SYS_SEQUENCES_ để lấy LAST_NUMBER
    // Đây là cách an toàn hơn, không cần khởi tạo sequence trong session
    {
        let conn = get_connection_with_retry(pool)?;
        let query_system = format!(
            "SELECT LAST_NUMBER FROM SYSTEM_.SYS_SEQUENCES_ WHERE USER_NAME = '{}' AND SEQUENCE_NAME = '{}'",
            schema, sequence_name
        );

        let cursor_result = conn.execute(&query_system, (), None)?;
        if let Some(mut cursor) = cursor_result {
            match cursor.next_row() {
                Ok(Some(mut row)) => {
                    let mut field = Nullable::<i64>::null();
                    match row.get_data(1, &mut field) {
                        Ok(_) => {
                            if let Some(last_number) = field.into_opt() {
                                return Ok(last_number);
                            }
                        }
                        Err(_) => {
                            // Nếu không lấy được từ system table, fallback về cách cũ
                        }
                    }
                }
                Ok(None) => {
                    // Không tìm thấy trong system table, fallback về cách cũ
                }
                Err(_) => {
                    // Lỗi query system table, fallback về cách cũ
                }
            }
        }
        // conn được drop ở đây
    }

    // Fallback: Dùng cách cũ với CURRVAL/NEXTVAL
    get_current_sequence_value(pool, &full_sequence_name)
}
