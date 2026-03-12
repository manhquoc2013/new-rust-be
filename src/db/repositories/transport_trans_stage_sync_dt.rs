//! Entity và repository for table TRANSPORT_TRANS_STAGE_SYNC_DT (chi tiết đồng bộ từ Hub).
//! Dùng khi ghi giao dịch từ sync sang bảng gốc, cần lưu cả chi tiết TCD.

use std::sync::Arc;

use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::configs::rating_db::RATING_DB;
use crate::db::{
    get_i64, get_nullable_datetime_string, get_nullable_i64, get_nullable_string, DbError,
    RowMapper,
};
use odbc_api::{Cursor, CursorRow};
use r2d2::Pool;

/// Entity for table TRANSPORT_TRANS_STAGE_SYNC_DT
#[derive(Debug, Clone)]
pub struct TransportTransStageSyncDt {
    pub transport_sync_dt_id: i64,
    pub transport_sync_id: i64,
    pub ticket_in_id: Option<i64>,
    pub ticket_etag_id: Option<i64>,
    pub ticket_out_id: Option<i64>,
    pub hub_id: Option<i64>,
    pub boo: Option<i64>,
    pub bot_id: Option<i64>,
    pub toll_a_id: Option<i64>,
    pub toll_b_id: Option<i64>,
    pub stage_id: Option<i64>,
    pub price_id: Option<i64>,
    pub ticket_type: Option<String>,
    pub price_ticket_type: Option<String>,
    pub price_amount: Option<i64>,
    pub subscription_id: Option<String>,
    pub vehicle_type: Option<i64>,
    pub checkin_datetime: Option<String>,
}

/// Mapper cho TRANSPORT_TRANS_STAGE_SYNC_DT
pub struct TransportTransStageSyncDtMapper;

impl RowMapper<TransportTransStageSyncDt> for TransportTransStageSyncDtMapper {
    fn map_row(&self, row: &mut CursorRow) -> Result<TransportTransStageSyncDt, DbError> {
        Ok(TransportTransStageSyncDt {
            transport_sync_dt_id: get_i64(row, 1)?,
            transport_sync_id: get_i64(row, 2)?,
            ticket_in_id: get_nullable_i64(row, 3)?,
            ticket_etag_id: get_nullable_i64(row, 4)?,
            ticket_out_id: get_nullable_i64(row, 5)?,
            hub_id: get_nullable_i64(row, 6)?,
            boo: get_nullable_i64(row, 7)?,
            bot_id: get_nullable_i64(row, 8)?,
            toll_a_id: get_nullable_i64(row, 9)?,
            toll_b_id: get_nullable_i64(row, 10)?,
            stage_id: get_nullable_i64(row, 11)?,
            price_id: get_nullable_i64(row, 12)?,
            ticket_type: get_nullable_string(row, 13)?,
            price_ticket_type: get_nullable_string(row, 14)?,
            price_amount: get_nullable_i64(row, 15)?,
            subscription_id: get_nullable_string(row, 16)?,
            vehicle_type: get_nullable_i64(row, 17)?,
            checkin_datetime: get_nullable_datetime_string(row, 18)?,
        })
    }
}

const TRANSPORT_TRANS_STAGE_SYNC_DT_SELECT: &str = "TRANSPORT_SYNC_DT_ID, TRANSPORT_SYNC_ID, TICKET_IN_ID, TICKET_ETAG_ID, TICKET_OUT_ID, HUB_ID, \
BOO, BOT_ID, TOLL_A_ID, TOLL_B_ID, STAGE_ID, PRICE_ID, TICKET_TYPE, PRICE_TICKET_TYPE, PRICE_AMOUNT, SUBSCRIPTION_ID, VEHICLE_TYPE, \
TO_CHAR(CHECKIN_DATETIME, 'YYYY-MM-DD HH24:MI:SS')";

/// Repository for TRANSPORT_TRANS_STAGE_SYNC_DT (chỉ đọc).
pub struct TransportTransStageSyncDtRepository {
    pool: Arc<Pool<OdbcConnectionManager>>,
    mapper: TransportTransStageSyncDtMapper,
}

impl TransportTransStageSyncDtRepository {
    pub fn new() -> Self {
        Self {
            pool: RATING_DB.clone(),
            mapper: TransportTransStageSyncDtMapper,
        }
    }

    /// Lấy tất cả chi tiết theo transport_sync_id.
    pub fn find_by_transport_sync_id(
        &self,
        transport_sync_id: i64,
    ) -> Result<Vec<TransportTransStageSyncDt>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!(
            "SELECT {} FROM RATING_OWNER.TRANSPORT_TRANS_STAGE_SYNC_DT WHERE TRANSPORT_SYNC_ID = {} ORDER BY TRANSPORT_SYNC_DT_ID",
            TRANSPORT_TRANS_STAGE_SYNC_DT_SELECT,
            transport_sync_id
        );
        let mapper = &self.mapper;
        let cursor_result = conn.execute(&query, (), None)?;
        match cursor_result {
            Some(mut cursor) => {
                let mut results = Vec::new();
                loop {
                    match cursor.next_row() {
                        Ok(Some(mut row)) => match mapper.map_row(&mut row) {
                            Ok(entity) => results.push(entity),
                            Err(e) => {
                                tracing::error!(error = %e, "[DB] TRANSPORT_TRANS_STAGE_SYNC_DT map_row failed");
                                continue;
                            }
                        },
                        Ok(None) => break,
                        Err(e) => {
                            return Err(DbError::ExecutionError(format!(
                                "Error fetching row: {}",
                                e
                            )));
                        }
                    }
                }
                Ok(results)
            }
            None => Ok(Vec::new()),
        }
    }

    fn get_pool(&self) -> &Pool<OdbcConnectionManager> {
        &self.pool
    }
}

impl Default for TransportTransStageSyncDtRepository {
    fn default() -> Self {
        Self::new()
    }
}
