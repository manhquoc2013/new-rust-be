//! Entity và repository for table TRANSPORT_TRANS_STAGE_SYNC (đồng bộ từ Hub Kafka).
//! Dùng khi checkout không tìm thấy giao dịch trong cache/KeyDB/DB để lấy đầu vào từ sync và ghi bảng gốc.

use std::sync::Arc;

use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::configs::rating_db::RATING_DB;
use crate::db::repository::escape_sql_string;
use crate::db::{
    get_i64, get_nullable_datetime_string, get_nullable_i64, get_nullable_string, DbError,
    RowMapper,
};
use odbc_api::{Cursor, CursorRow};
use r2d2::Pool;

/// Entity for table TRANSPORT_TRANS_STAGE_SYNC
#[derive(Debug, Clone)]
pub struct TransportTransStageSync {
    pub transport_sync_id: i64,
    pub ticket_in_id: Option<i64>,
    pub ticket_etag_id: Option<i64>,
    pub ticket_out_id: Option<i64>,
    pub hub_id: Option<i64>,
    pub checkin_toll_id: Option<i64>,
    pub checkin_lane_id: Option<i64>,
    pub checkin_datetime: Option<String>,
    pub checkin_commit_datetime: Option<String>,
    pub checkout_toll_id: Option<i64>,
    pub checkout_lane_id: Option<i64>,
    pub checkout_datetime: Option<String>,
    pub checkout_commit_datetime: Option<String>,
    pub total_amount: Option<i64>,
    pub etag_number: Option<String>,
    pub tid: Option<String>,
    pub request_id: Option<i64>,
    pub status: Option<i64>,
    pub sync_status: Option<i64>,
    pub insert_datetime: Option<String>,
    pub update_datetime: Option<String>,
    pub plate: Option<String>,
    pub vehicle_type: Option<String>,
    pub ticket_type: Option<String>,
    pub price_ticket_type: Option<String>,
    pub register_vehicle_type: Option<String>,
    pub seat: Option<i64>,
    pub weight_goods: Option<i64>,
    pub weight_all: Option<i64>,
    pub boo_etag: Option<String>,
    pub rating_type: Option<String>,
}

/// Mapper cho TRANSPORT_TRANS_STAGE_SYNC
pub struct TransportTransStageSyncMapper;

impl RowMapper<TransportTransStageSync> for TransportTransStageSyncMapper {
    fn map_row(&self, row: &mut CursorRow) -> Result<TransportTransStageSync, DbError> {
        Ok(TransportTransStageSync {
            transport_sync_id: get_i64(row, 1)?,
            ticket_in_id: get_nullable_i64(row, 2)?,
            ticket_etag_id: get_nullable_i64(row, 3)?,
            ticket_out_id: get_nullable_i64(row, 4)?,
            hub_id: get_nullable_i64(row, 5)?,
            checkin_toll_id: get_nullable_i64(row, 6)?,
            checkin_lane_id: get_nullable_i64(row, 7)?,
            checkin_datetime: get_nullable_datetime_string(row, 8)?,
            checkin_commit_datetime: get_nullable_datetime_string(row, 9)?,
            checkout_toll_id: get_nullable_i64(row, 10)?,
            checkout_lane_id: get_nullable_i64(row, 11)?,
            checkout_datetime: get_nullable_datetime_string(row, 12)?,
            checkout_commit_datetime: get_nullable_datetime_string(row, 13)?,
            total_amount: get_nullable_i64(row, 14)?,
            etag_number: get_nullable_string(row, 15)?,
            tid: get_nullable_string(row, 16)?,
            request_id: get_nullable_i64(row, 17)?,
            status: get_nullable_i64(row, 18)?,
            sync_status: get_nullable_i64(row, 19)?,
            insert_datetime: get_nullable_datetime_string(row, 20)?,
            update_datetime: get_nullable_datetime_string(row, 21)?,
            plate: get_nullable_string(row, 22)?,
            vehicle_type: get_nullable_string(row, 23)?,
            ticket_type: get_nullable_string(row, 24)?,
            price_ticket_type: get_nullable_string(row, 25)?,
            register_vehicle_type: get_nullable_string(row, 26)?,
            seat: get_nullable_i64(row, 27)?,
            weight_goods: get_nullable_i64(row, 28)?,
            weight_all: get_nullable_i64(row, 29)?,
            boo_etag: get_nullable_string(row, 30)?,
            rating_type: get_nullable_string(row, 31)?,
        })
    }
}

const TRANSPORT_TRANS_STAGE_SYNC_SELECT: &str = "TRANSPORT_SYNC_ID, TICKET_IN_ID, TICKET_ETAG_ID, TICKET_OUT_ID, HUB_ID, \
CHECKIN_TOLL_ID, CHECKIN_LANE_ID, TO_CHAR(CHECKIN_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), TO_CHAR(CHECKIN_COMMIT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), \
CHECKOUT_TOLL_ID, CHECKOUT_LANE_ID, TO_CHAR(CHECKOUT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), TO_CHAR(CHECKOUT_COMMIT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), \
TOTAL_AMOUNT, ETAG_NUMBER, TID, REQUEST_ID, STATUS, SYNC_STATUS, \
TO_CHAR(INSERT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), TO_CHAR(UPDATE_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), \
PLATE, VEHICLE_TYPE, TICKET_TYPE, PRICE_TICKET_TYPE, REGISTER_VEHICLE_TYPE, SEAT, WEIGHT_GOODS, WEIGHT_ALL, BOO_ETAG, RATING_TYPE";

/// Repository for TRANSPORT_TRANS_STAGE_SYNC (chỉ đọc).
pub struct TransportTransStageSyncRepository {
    pool: Arc<Pool<OdbcConnectionManager>>,
    mapper: TransportTransStageSyncMapper,
}

impl TransportTransStageSyncRepository {
    pub fn new() -> Self {
        Self {
            pool: RATING_DB.clone(),
            mapper: TransportTransStageSyncMapper,
        }
    }

    /// Get record sync latest by etag (theo CHECKIN_DATETIME), không lọc theo checkout.
    /// Dùng trong merge "latest by etag" (sync + boo/ttt + cache); handler kiểm tra điều kiện sau.
    pub fn find_latest_by_etag(
        &self,
        etag: &str,
    ) -> Result<Option<TransportTransStageSync>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let escaped = escape_sql_string(etag.trim());
        let query = format!(
            "SELECT * FROM (SELECT {} FROM RATING_OWNER.TRANSPORT_TRANS_STAGE_SYNC \
             WHERE ETAG_NUMBER = '{}' AND STATUS IN (0, 1) AND RATING_TYPE = 'E' \
             ORDER BY CHECKIN_DATETIME DESC, TRANSPORT_SYNC_ID DESC) WHERE ROWNUM = 1",
            TRANSPORT_TRANS_STAGE_SYNC_SELECT, escaped
        );
        let mapper = &self.mapper;
        let cursor_result = conn.execute(&query, (), None)?;
        match cursor_result {
            Some(mut cursor) => match cursor.next_row() {
                Ok(Some(mut row)) => mapper.map_row(&mut row).map(Some),
                Ok(None) => Ok(None),
                Err(e) => Err(DbError::ExecutionError(format!(
                    "Error fetching row: {}",
                    e
                ))),
            },
            None => Ok(None),
        }
    }

    /// Lấy một bản ghi sync chưa checkout theo etag (bản ghi latest by thời gian insert).
    /// Điều kiện: TRIM(ETAG_NUMBER) = etag; CHECKOUT_DATETIME IS NULL; STATUS IN (0, 1); RATING_TYPE = 'E'.
    /// Sắp xếp theo CHECKIN_DATETIME DESC, TRANSPORT_SYNC_ID DESC để lấy record sync mới nhất.
    pub fn find_by_etag_pending_checkout(
        &self,
        etag: &str,
    ) -> Result<Option<TransportTransStageSync>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let escaped = escape_sql_string(etag.trim());
        let query = format!(
            "SELECT * FROM (SELECT {} FROM RATING_OWNER.TRANSPORT_TRANS_STAGE_SYNC \
             WHERE ETAG_NUMBER = '{}' AND CHECKOUT_DATETIME IS NULL AND STATUS IN (0, 1) AND RATING_TYPE = 'E' \
             ORDER BY CHECKIN_DATETIME DESC, TRANSPORT_SYNC_ID DESC) WHERE ROWNUM = 1",
            TRANSPORT_TRANS_STAGE_SYNC_SELECT,
            escaped
        );
        let mapper = &self.mapper;
        let cursor_result = conn.execute(&query, (), None)?;
        match cursor_result {
            Some(mut cursor) => match cursor.next_row() {
                Ok(Some(mut row)) => mapper.map_row(&mut row).map(Some),
                Ok(None) => Ok(None),
                Err(e) => Err(DbError::ExecutionError(format!(
                    "Error fetching row: {}",
                    e
                ))),
            },
            None => Ok(None),
        }
    }

    fn get_pool(&self) -> &Pool<OdbcConnectionManager> {
        &self.pool
    }
}

impl Default for TransportTransStageSyncRepository {
    fn default() -> Self {
        Self::new()
    }
}
