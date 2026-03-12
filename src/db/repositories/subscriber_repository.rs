//! Look up subscriber info (SubscriberInfo) by subscriber_id.

use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::DbError;
use odbc_api::{Cursor, IntoParameter};
use r2d2::Pool;

#[derive(Debug, Clone)]
pub struct SubscriberInfo {
    pub subscriber_id: i64,
    pub account_id: i64,
    pub vehicle_id: i64,
    pub etag_id: i64,
    pub plate: String,
    pub vehicle_type: String,
}

pub fn get_subscriber_by_etag(
    pool: &Pool<OdbcConnectionManager>,
    etag_number: &str,
) -> Result<Option<SubscriberInfo>, DbError> {
    let conn = get_connection_with_retry(pool).map_err(|e| DbError::PoolError(e.to_string()))?;

    // Query similar to be-trans but with only the required fields
    let sql = r#"
        SELECT S.SUBSCRIBER_ID, S.ACCOUNT_ID, S.VEHICLE_ID, E.ETAG_ID, V.PLATE, NVL(V.VEHICLE_TYPE,'-1')
        FROM CRM_OWNER.SUBSCRIBER S, CRM_OWNER.VEHICLE V ,CRM_OWNER.ETAG E
        WHERE S.VEHICLE_ID = V.VEHICLE_ID AND S.ETAG_ID = E.ETAG_ID AND E.ETAG_NUMBER = ?
    "#;

    let param = etag_number.into_parameter();
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
                let mut subscriber_id: i64 = 0;
                r.get_data(1, &mut subscriber_id)
                    .map_err(|e| DbError::ConversionError(format!("subscriber_id: {}", e)))?;

                let mut account_id: i64 = 0;
                r.get_data(2, &mut account_id)
                    .map_err(|e| DbError::ConversionError(format!("account_id: {}", e)))?;

                let mut vehicle_id: i64 = 0;
                r.get_data(3, &mut vehicle_id)
                    .map_err(|e| DbError::ConversionError(format!("vehicle_id: {}", e)))?;

                let mut etag_id: i64 = 0;
                r.get_data(4, &mut etag_id)
                    .map_err(|e| DbError::ConversionError(format!("etag_id: {}", e)))?;

                // Plate (col 5)
                let mut plate_buf = Vec::new();
                r.get_text(5, &mut plate_buf)
                    .map_err(|e| DbError::ConversionError(format!("plate: {}", e)))?;
                let plate = String::from_utf8_lossy(&plate_buf)
                    .trim_end_matches('\0')
                    .to_string();

                // Vehicle Type (col 6)
                let mut vtype_buf = Vec::new();
                r.get_text(6, &mut vtype_buf)
                    .map_err(|e| DbError::ConversionError(format!("vehicle_type: {}", e)))?;
                let vehicle_type = String::from_utf8_lossy(&vtype_buf)
                    .trim_end_matches('\0')
                    .to_string();

                Ok(Some(SubscriberInfo {
                    subscriber_id,
                    account_id,
                    vehicle_id,
                    etag_id,
                    plate,
                    vehicle_type,
                }))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}
