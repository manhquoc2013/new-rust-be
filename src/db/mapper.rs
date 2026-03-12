//! RowMapper trait and helpers to read columns from ODBC row (string, i32, i64, datetime, f64).

use crate::db::error::DbError;
use crate::db::repository::normalize_datetime_string;
use odbc_api::CursorRow;

/// Trait to map DB row to entity.
pub trait RowMapper<T> {
    fn map_row(&self, row: &mut CursorRow) -> Result<T, DbError>;
}

/// Read nullable string column from row (get_text, odbc-api 14.1.0).
pub fn get_nullable_string(
    row: &mut odbc_api::CursorRow,
    col_index: u16,
) -> Result<Option<String>, DbError> {
    let mut buffer: Vec<u8> = vec![0u8; 4000];
    match row.get_text(col_index, &mut buffer) {
        Ok(has_data) => {
            if has_data {
                let len = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
                let text = String::from_utf8_lossy(&buffer[..len]).trim().to_string();

                if text.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(text))
                }
            } else {
                Ok(None)
            }
        }
        Err(e) => {
            tracing::warn!(column = col_index, error = %e, "[DB] get_nullable_string error, returning None");
            Ok(None)
        }
    }
}

/// Read nullable datetime column and normalize to 'YYYY-MM-DD HH24:MI:SS'.
pub fn get_nullable_datetime_string(
    row: &mut odbc_api::CursorRow,
    col_index: u16,
) -> Result<Option<String>, DbError> {
    get_nullable_string(row, col_index).map(|opt| opt.map(|s| normalize_datetime_string(&s)))
}

/// Read nullable i32 column from row.
pub fn get_nullable_i32(
    row: &mut odbc_api::CursorRow,
    col_index: u16,
) -> Result<Option<i32>, DbError> {
    use odbc_api::Nullable;
    let mut field = Nullable::<i32>::null();
    row.get_data(col_index, &mut field).map_err(|e| {
        DbError::ConversionError(format!("Failed to get i32 at column {}: {}", col_index, e))
    })?;
    Ok(field.into_opt())
}

/// Read nullable i64 column from row.
pub fn get_nullable_i64(
    row: &mut odbc_api::CursorRow,
    col_index: u16,
) -> Result<Option<i64>, DbError> {
    use odbc_api::Nullable;
    let mut field = Nullable::<i64>::null();
    row.get_data(col_index, &mut field).map_err(|e| {
        DbError::ConversionError(format!("Failed to get i64 at column {}: {}", col_index, e))
    })?;
    Ok(field.into_opt())
}

/// Read required string column from row.
pub fn get_string(row: &mut odbc_api::CursorRow, col_index: u16) -> Result<String, DbError> {
    get_nullable_string(row, col_index)?.ok_or_else(|| {
        DbError::ConversionError(format!("Column {} is null but expected string", col_index))
    })
}

/// Read required i32 column from row.
pub fn get_i32(row: &mut odbc_api::CursorRow, col_index: u16) -> Result<i32, DbError> {
    get_nullable_i32(row, col_index)?.ok_or_else(|| {
        DbError::ConversionError(format!("Column {} is null but expected i32", col_index))
    })
}

/// Read required i64 column from row.
pub fn get_i64(row: &mut odbc_api::CursorRow, col_index: u16) -> Result<i64, DbError> {
    get_nullable_i64(row, col_index)?.ok_or_else(|| {
        DbError::ConversionError(format!("Column {} is null but expected i64", col_index))
    })
}

/// Read nullable f64 column from row.
pub fn get_nullable_f64(
    row: &mut odbc_api::CursorRow,
    col_index: u16,
) -> Result<Option<f64>, DbError> {
    use odbc_api::Nullable;
    let mut field = Nullable::<f64>::null();
    row.get_data(col_index, &mut field).map_err(|e| {
        DbError::ConversionError(format!("Failed to get f64 at column {}: {}", col_index, e))
    })?;
    Ok(field.into_opt())
}
