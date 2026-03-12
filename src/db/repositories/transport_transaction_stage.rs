//! Entity, mapper, repository for table TRANSPORT_TRANSACTION_STAGE.

use std::sync::Arc;

use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::configs::rating_db::RATING_DB;
use crate::db::repository::{format_sql_datetime_str, format_sql_nullable_datetime};
use crate::db::{
    get_i64, get_nullable_datetime_string, get_nullable_i32, get_nullable_i64, get_nullable_string,
    DbError, Repository, RowMapper,
};
use odbc_api::{Cursor, CursorRow};
use r2d2::Pool;

/// Entity for table TRANSPORT_TRANSACTION_STAGE
#[derive(Debug, Clone)]
pub struct TransportTransactionStage {
    pub transport_trans_id: i64,      // TRANSPORT_TRANS_ID - Primary Key
    pub subscriber_id: Option<i64>,   // SUBSCRIBER_ID
    pub etag_id: Option<i64>,         // ETAG_ID
    pub vehicle_id: Option<i64>,      // VEHICLE_ID
    pub checkin_toll_id: Option<i64>, // CHECKIN_TOLL_ID
    pub checkin_lane_id: Option<i64>, // CHECKIN_LANE_ID
    pub checkin_commit_datetime: Option<String>, // CHECKIN_COMMIT_DATETIME
    pub checkin_channel: Option<i64>, // CHECKIN_CHANNEL
    pub checkin_pass: Option<String>, // CHECKIN_PASS
    pub checkin_pass_reason_id: Option<String>, // CHECKIN_PASS_REASON_ID
    pub checkout_toll_id: Option<i64>, // CHECKOUT_TOLL_ID
    pub checkout_lane_id: Option<i64>, // CHECKOUT_LANE_ID
    pub checkout_commit_datetime: Option<String>, // CHECKOUT_COMMIT_DATETIME
    pub checkout_channel: Option<i64>, // CHECKOUT_CHANNEL
    pub checkout_pass: Option<String>, // CHECKOUT_PASS
    pub checkout_pass_reason_id: Option<String>, // CHECKOUT_PASS_REASON_ID
    pub charge_status: Option<String>, // CHARGE_STATUS
    pub charge_type: Option<String>,  // CHARGE_TYPE
    pub total_amount: Option<i64>,    // TOTAL_AMOUNT
    pub account_id: Option<i64>,      // ACCOUNT_ID
    pub account_trans_id: Option<i64>, // ACCOUNT_TRANS_ID
    pub checkin_datetime: Option<String>, // CHECKIN_DATETIME
    pub checkout_datetime: Option<String>, // CHECKOUT_DATETIME
    pub checkin_status: Option<String>, // CHECKIN_STATUS
    pub etag_number: Option<String>,  // ETAG_NUMBER
    pub request_id: Option<i64>,      // REQUEST_ID
    pub checkin_plate: Option<String>, // CHECKIN_PLATE
    pub checkin_plate_status: Option<String>, // CHECKIN_PLATE_STATUS
    pub checkout_plate: Option<String>, // CHECKOUT_PLATE
    pub checkout_plate_status: Option<String>, // CHECKOUT_PLATE_STATUS
    pub plate_from_toll: Option<String>, // PLATE_FROM_TOLL
    pub img_count: Option<i32>,       // IMG_COUNT
    pub checkin_img_count: Option<i32>, // CHECKIN_IMG_COUNT
    pub checkout_img_count: Option<i32>, // CHECKOUT_IMG_COUNT
    pub status: Option<String>,       // STATUS
    pub last_update: Option<String>,  // LAST_UPDATE
    pub notif_scan: Option<i32>,      // NOTIF_SCAN
    pub toll_type: Option<String>,    // TOLL_TYPE
    pub rating_type: Option<String>,  // RATING_TYPE
    pub insert_datetime: Option<String>, // INSERT_DATETIME
    pub insuff_amount: Option<f64>,   // INSUFF_AMOUNT (NUMERIC(12,2))
    pub checkout_status: Option<String>, // CHECKOUT_STATUS
    pub bitwise_scan: Option<i64>,    // BITWISE_SCAN
    pub plate: Option<String>,        // PLATE
    pub vehicle_type: Option<String>, // VEHICLE_TYPE
    pub fe_vehicle_length: Option<i64>, // FE_VEHICLE_LENGTH
    pub fe_commit_amount: Option<i64>, // FE_COMMIT_AMOUNT
    pub fe_weight: Option<i64>,       // FE_WEIGHT
    pub fe_reason_id: Option<i64>,    // FE_REASON_ID
    pub vehicle_type_profile: Option<String>, // VEHICLE_TYPE_PROFILE
    pub boo: Option<i64>,             // BOO
    pub boo_transport_trans_id: Option<i64>, // BOO_TRANSPORT_TRANS_ID
    pub subscription_ids: Option<String>, // SUBSCRIPTION_IDS
    pub checkin_shift: Option<String>, // CHECKIN_SHIFT
    pub checkout_shift: Option<String>, // CHECKOUT_SHIFT
    pub turning_code: Option<String>, // TURNING_CODE
    pub checkin_tid: Option<String>,  // CHECKIN_TID
    pub checkout_tid: Option<String>, // CHECKOUT_TID
    pub charge_in: Option<String>,    // CHARGE_IN
    pub charge_trans_id: Option<String>, // CHARGE_TRANS_ID
    pub balance: Option<f64>,         // BALANCE (NUMERIC(20,2))
    pub charge_in_status: Option<String>, // CHARGE_IN_STATUS
    pub charge_datetime: Option<String>, // CHARGE_DATETIME
    pub charge_in_104: Option<String>, // CHARGE_IN_104
    pub fe_online_status: Option<String>, // FE_ONLINE_STATUS
    pub mdh_id: Option<i64>,          // MDH_ID
    pub chd_type: Option<String>,     // CHD_TYPE
    pub chd_ref_id: Option<i64>,      // CHD_REF_ID
    pub chd_reason: Option<String>,   // CHD_REASON
    pub fe_trans_id: Option<String>,  // FE_TRANS_ID
    pub transition_close: Option<String>, // TRANSITION_CLOSE
    pub voucher_code: Option<String>, // VOUCHER_CODE
    pub voucher_used_amount: Option<i64>, // VOUCHER_USED_AMOUNT
    pub voucher_amount: Option<i64>,  // VOUCHER_AMOUNT
    pub transport_sync_id: Option<i64>, // TRANSPORT_SYNC_ID
    pub ticket_in_id: Option<i64>,    // TICKET_IN_ID
    pub hub_id: Option<i64>,          // HUB_ID
    pub ticket_eTag_id: Option<i64>,  // TICKET_ETAG_ID
    pub ticket_out_id: Option<i64>,   // TICKET_OUT_ID
    pub token_id: Option<String>,     // TOKEN_ID
    pub acs_account_no: Option<String>, // ACS_ACCOUNT_NO
    pub boo_etag: Option<String>,     // BOO_ETAG
    pub sub_charge_in: Option<String>, // SUB_CHARGE_IN
    pub sync_status: Option<i64>, // SYNC_STATUS (0/None = chưa đồng bộ, 1 = đã đồng bộ; commit → 0)
}

/// Mapper cho TRANSPORT_TRANSACTION_STAGE
pub struct TransportTransactionStageMapper;

impl RowMapper<TransportTransactionStage> for TransportTransactionStageMapper {
    fn map_row(&self, row: &mut CursorRow) -> Result<TransportTransactionStage, DbError> {
        use crate::db::get_nullable_f64;

        Ok(TransportTransactionStage {
            transport_trans_id: get_i64(row, 1)?,       // TRANSPORT_TRANS_ID
            subscriber_id: get_nullable_i64(row, 2)?,   // SUBSCRIBER_ID
            etag_id: get_nullable_i64(row, 3)?,         // ETAG_ID
            vehicle_id: get_nullable_i64(row, 4)?,      // VEHICLE_ID
            checkin_toll_id: get_nullable_i64(row, 5)?, // CHECKIN_TOLL_ID
            checkin_lane_id: get_nullable_i64(row, 6)?, // CHECKIN_LANE_ID
            checkin_commit_datetime: get_nullable_datetime_string(row, 7)?, // CHECKIN_COMMIT_DATETIME
            checkin_channel: get_nullable_i64(row, 8)?,                     // CHECKIN_CHANNEL
            checkin_pass: get_nullable_string(row, 9)?,                     // CHECKIN_PASS
            checkin_pass_reason_id: get_nullable_string(row, 10)?, // CHECKIN_PASS_REASON_ID
            checkout_toll_id: get_nullable_i64(row, 11)?,          // CHECKOUT_TOLL_ID
            checkout_lane_id: get_nullable_i64(row, 12)?,          // CHECKOUT_LANE_ID
            checkout_commit_datetime: get_nullable_datetime_string(row, 13)?, // CHECKOUT_COMMIT_DATETIME
            checkout_channel: get_nullable_i64(row, 14)?,                     // CHECKOUT_CHANNEL
            checkout_pass: get_nullable_string(row, 15)?,                     // CHECKOUT_PASS
            checkout_pass_reason_id: get_nullable_string(row, 16)?, // CHECKOUT_PASS_REASON_ID
            charge_status: get_nullable_string(row, 17)?,           // CHARGE_STATUS
            charge_type: get_nullable_string(row, 18)?,             // CHARGE_TYPE
            total_amount: get_nullable_i64(row, 19)?,               // TOTAL_AMOUNT
            account_id: get_nullable_i64(row, 20)?,                 // ACCOUNT_ID
            account_trans_id: get_nullable_i64(row, 21)?,           // ACCOUNT_TRANS_ID
            checkin_datetime: get_nullable_datetime_string(row, 22)?, // CHECKIN_DATETIME
            checkout_datetime: get_nullable_datetime_string(row, 23)?, // CHECKOUT_DATETIME
            checkin_status: get_nullable_string(row, 24)?,          // CHECKIN_STATUS
            etag_number: get_nullable_string(row, 25)?,             // ETAG_NUMBER
            request_id: get_nullable_i64(row, 26)?,                 // REQUEST_ID
            checkin_plate: get_nullable_string(row, 27)?,           // CHECKIN_PLATE
            checkin_plate_status: get_nullable_string(row, 28)?,    // CHECKIN_PLATE_STATUS
            checkout_plate: get_nullable_string(row, 29)?,          // CHECKOUT_PLATE
            checkout_plate_status: get_nullable_string(row, 30)?,   // CHECKOUT_PLATE_STATUS
            plate_from_toll: get_nullable_string(row, 31)?,         // PLATE_FROM_TOLL
            img_count: get_nullable_i32(row, 32)?,                  // IMG_COUNT
            checkin_img_count: get_nullable_i32(row, 33)?,          // CHECKIN_IMG_COUNT
            checkout_img_count: get_nullable_i32(row, 34)?,         // CHECKOUT_IMG_COUNT
            status: get_nullable_string(row, 35)?,                  // STATUS
            last_update: get_nullable_datetime_string(row, 36)?,    // LAST_UPDATE
            notif_scan: get_nullable_i32(row, 37)?,                 // NOTIF_SCAN
            toll_type: get_nullable_string(row, 38)?,               // TOLL_TYPE
            rating_type: get_nullable_string(row, 39)?,             // RATING_TYPE
            insert_datetime: get_nullable_datetime_string(row, 40)?, // INSERT_DATETIME
            insuff_amount: get_nullable_f64(row, 41)?,              // INSUFF_AMOUNT
            checkout_status: get_nullable_string(row, 42)?,         // CHECKOUT_STATUS
            bitwise_scan: get_nullable_i64(row, 43)?,               // BITWISE_SCAN
            plate: get_nullable_string(row, 44)?,                   // PLATE
            vehicle_type: get_nullable_string(row, 45)?,            // VEHICLE_TYPE
            fe_vehicle_length: get_nullable_i64(row, 46)?,          // FE_VEHICLE_LENGTH
            fe_commit_amount: get_nullable_i64(row, 47)?,           // FE_COMMIT_AMOUNT
            fe_weight: get_nullable_i64(row, 48)?,                  // FE_WEIGHT
            fe_reason_id: get_nullable_i64(row, 49)?,               // FE_REASON_ID
            vehicle_type_profile: get_nullable_string(row, 50)?,    // VEHICLE_TYPE_PROFILE
            boo: get_nullable_i64(row, 51)?,                        // BOO
            boo_transport_trans_id: get_nullable_i64(row, 52)?,     // BOO_TRANSPORT_TRANS_ID
            subscription_ids: get_nullable_string(row, 53)?,        // SUBSCRIPTION_IDS
            checkin_shift: get_nullable_string(row, 54)?,           // CHECKIN_SHIFT
            checkout_shift: get_nullable_string(row, 55)?,          // CHECKOUT_SHIFT
            turning_code: get_nullable_string(row, 56)?,            // TURNING_CODE
            checkin_tid: get_nullable_string(row, 57)?,             // CHECKIN_TID
            checkout_tid: get_nullable_string(row, 58)?,            // CHECKOUT_TID
            charge_in: get_nullable_string(row, 59)?,               // CHARGE_IN
            charge_trans_id: get_nullable_string(row, 60)?,         // CHARGE_TRANS_ID
            balance: get_nullable_f64(row, 61)?,                    // BALANCE
            charge_in_status: get_nullable_string(row, 62)?,        // CHARGE_IN_STATUS
            charge_datetime: get_nullable_datetime_string(row, 63)?, // CHARGE_DATETIME
            charge_in_104: get_nullable_string(row, 64)?,           // CHARGE_IN_104
            fe_online_status: get_nullable_string(row, 65)?,        // FE_ONLINE_STATUS
            mdh_id: get_nullable_i64(row, 66)?,                     // MDH_ID
            chd_type: get_nullable_string(row, 67)?,                // CHD_TYPE
            chd_ref_id: get_nullable_i64(row, 68)?,                 // CHD_REF_ID
            chd_reason: get_nullable_string(row, 69)?,              // CHD_REASON
            fe_trans_id: get_nullable_string(row, 70)?,             // FE_TRANS_ID
            transition_close: get_nullable_string(row, 71)?,        // TRANSITION_CLOSE
            voucher_code: get_nullable_string(row, 72)?,            // VOUCHER_CODE
            voucher_used_amount: get_nullable_i64(row, 73)?,        // VOUCHER_USED_AMOUNT
            voucher_amount: get_nullable_i64(row, 74)?,             // VOUCHER_AMOUNT
            transport_sync_id: get_nullable_i64(row, 75)?,          // TRANSPORT_SYNC_ID
            ticket_in_id: get_nullable_i64(row, 76)?,               // TICKET_IN_ID
            hub_id: get_nullable_i64(row, 77)?,                     // HUB_ID
            ticket_eTag_id: get_nullable_i64(row, 78)?,             // TICKET_ETAG_ID
            ticket_out_id: get_nullable_i64(row, 79)?,              // TICKET_OUT_ID
            token_id: get_nullable_string(row, 80)?,                // TOKEN_ID
            acs_account_no: get_nullable_string(row, 81)?,          // ACS_ACCOUNT_NO
            boo_etag: get_nullable_string(row, 82)?,                // BOO_ETAG
            sub_charge_in: get_nullable_string(row, 83)?,           // SUB_CHARGE_IN
            sync_status: get_nullable_i64(row, 84)?,                // SYNC_STATUS
        })
    }
}

/// SELECT list with TO_CHAR for datetime columns so when reading via ODBC the time part is preserved (Altibase/ODBC returns DATE as date-only string if using raw column).
const TRANSPORT_TRANSACTION_STAGE_SELECT: &str = "TRANSPORT_TRANS_ID, SUBSCRIBER_ID, ETAG_ID, VEHICLE_ID, CHECKIN_TOLL_ID, CHECKIN_LANE_ID, \
TO_CHAR(CHECKIN_COMMIT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), CHECKIN_CHANNEL, CHECKIN_PASS, CHECKIN_PASS_REASON_ID, \
CHECKOUT_TOLL_ID, CHECKOUT_LANE_ID, TO_CHAR(CHECKOUT_COMMIT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), CHECKOUT_CHANNEL, CHECKOUT_PASS, CHECKOUT_PASS_REASON_ID, CHARGE_STATUS, CHARGE_TYPE, TOTAL_AMOUNT, ACCOUNT_ID, ACCOUNT_TRANS_ID, \
TO_CHAR(CHECKIN_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), TO_CHAR(CHECKOUT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), CHECKIN_STATUS, ETAG_NUMBER, REQUEST_ID, CHECKIN_PLATE, CHECKIN_PLATE_STATUS, CHECKOUT_PLATE, CHECKOUT_PLATE_STATUS, \
PLATE_FROM_TOLL, IMG_COUNT, CHECKIN_IMG_COUNT, CHECKOUT_IMG_COUNT, STATUS, TO_CHAR(LAST_UPDATE, 'YYYY-MM-DD HH24:MI:SS'), NOTIF_SCAN, TOLL_TYPE, RATING_TYPE, TO_CHAR(INSERT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), \
INSUFF_AMOUNT, CHECKOUT_STATUS, CAST(NULL AS BIGINT), PLATE, VEHICLE_TYPE, FE_VEHICLE_LENGTH, FE_COMMIT_AMOUNT, FE_WEIGHT, FE_REASON_ID, VEHICLE_TYPE_PROFILE, \
BOO, BOO_TRANSPORT_TRANS_ID, CAST(NULL AS VARCHAR(20)), CHECKIN_SHIFT, CHECKOUT_SHIFT, CAST(NULL AS VARCHAR(20)), CHECKIN_TID, CHECKOUT_TID, CHARGE_IN, CHARGE_TRANS_ID, \
BALANCE, CHARGE_IN_STATUS, TO_CHAR(CHARGE_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), CHARGE_IN_104, FE_ONLINE_STATUS, CAST(NULL AS BIGINT), CHD_TYPE, CHD_REF_ID, CHD_REASON, FE_TRANS_ID, \
TRANSITION_CLOSE, VOUCHER_CODE, VOUCHER_USED_AMOUNT, VOUCHER_AMOUNT, TRANSPORT_SYNC_ID, TICKET_IN_ID, HUB_ID, TICKET_ETAG_ID, TICKET_OUT_ID, \
CAST(NULL AS VARCHAR(20)), CAST(NULL AS VARCHAR(20)), CAST(NULL AS VARCHAR(20)), CAST(NULL AS VARCHAR(20)), SYNC_STATUS";

/// Repository cho TRANSPORT_TRANSACTION_STAGE
pub struct TransportTransactionStageRepository {
    pool: Arc<Pool<OdbcConnectionManager>>,
    mapper: TransportTransactionStageMapper,
}

impl TransportTransactionStageRepository {
    pub fn new() -> Self {
        Self {
            pool: RATING_DB.clone(),
            mapper: TransportTransactionStageMapper,
        }
    }

    /// Cập nhật các trường cần thiết cho checkout commit (BECT).
    /// Chỉ các trường liên quan; không ghi đè datetime khác. checkout_plate_status: khi commit lỗi set "0" (thống nhất BOO).
    /// checkout_plate: khi Some, cập nhật CHECKOUT_PLATE và PLATE (đồng bộ với fe_resp, lưu BKS kể cả "0000000000").
    /// sync_status: khi commit giao dịch truyền Some(0) để set về not synced.
    pub fn update_checkout_commit(
        &self,
        transport_trans_id: i64,
        checkout_commit_datetime: &str,
        checkout_img_count: i32,
        last_update: &str,
        charge_status: &str,
        checkout_status: Option<&str>,
        fe_reason_id: Option<i64>,
        checkout_datetime: Option<&str>,
        img_count: Option<i32>,
        checkout_plate_status: Option<&str>,
        checkout_plate: Option<&str>,
        sync_status: Option<i64>,
    ) -> Result<bool, DbError> {
        let pool = &self.pool;
        let conn =
            get_connection_with_retry(pool).map_err(|e| DbError::PoolError(e.to_string()))?;

        // Dựng SET clauses động (format_sql_datetime_str chuẩn hóa, tránh mất phần giờ).
        let mut set_parts = vec![
            format!(
                "CHECKOUT_COMMIT_DATETIME = {}",
                format_sql_datetime_str(checkout_commit_datetime)
            ),
            format!("CHECKOUT_IMG_COUNT = {}", checkout_img_count),
            format!("LAST_UPDATE = {}", format_sql_datetime_str(last_update)),
            format!("CHARGE_STATUS = '{}'", charge_status),
        ];

        if let Some(cp) = checkout_plate {
            let escaped = cp.replace("'", "''");
            set_parts.push(format!("CHECKOUT_PLATE = '{}'", escaped));
            set_parts.push(format!("PLATE = '{}'", escaped));
        }

        if let Some(cs) = checkout_status {
            set_parts.push(format!("CHECKOUT_STATUS = '{}'", cs));
        }

        if let Some(r) = fe_reason_id {
            set_parts.push(format!("FE_REASON_ID = {}", r));
        }

        if let Some(cps) = checkout_plate_status {
            set_parts.push(format!(
                "CHECKOUT_PLATE_STATUS = '{}'",
                cps.replace("'", "''")
            ));
        }

        // Cập nhật checkout_datetime nếu được cung cấp (đồng bộ với BOO)
        if let Some(cdt) = checkout_datetime {
            set_parts.push(format!(
                "CHECKOUT_DATETIME = {}",
                format_sql_datetime_str(cdt)
            ));
        }

        if let Some(ic) = img_count {
            set_parts.push(format!("IMG_COUNT = {}", ic));
        }

        if let Some(ss) = sync_status {
            set_parts.push(format!("SYNC_STATUS = {}", ss));
        }

        let sql = format!(
            "UPDATE RATING_OWNER.TRANSPORT_TRANSACTION_STAGE SET {} WHERE TRANSPORT_TRANS_ID = {}",
            set_parts.join(", "),
            transport_trans_id
        );

        tracing::debug!(
            transport_trans_id,
            sql_len = sql.len(),
            "[DB] update_checkout_commit"
        );
        // Oracle (và nhiều driver ODBC) không trả cursor cho UPDATE → execute() trả None; coi là thành công nếu không lỗi.
        conn.execute(&sql, (), None)?;
        Ok(true)
    }

    /// Update HUB_ID by REQUEST_ID (từ bản tin đồng bộ CHECKIN_HUB_INFO HUB → BOO, BECT).
    pub fn update_hub_id_by_request_id(
        &self,
        request_id: i64,
        hub_id: i64,
    ) -> Result<u64, DbError> {
        use crate::db::format_sql_value;
        use crate::db::repository::{format_sql_nullable_datetime, now_utc_for_db};
        let conn = self
            .pool
            .get()
            .map_err(|e| DbError::PoolError(e.to_string()))?;
        let now = now_utc_for_db();
        let last_update = format_sql_nullable_datetime(&Some(now));
        let query = format!(
            "UPDATE {} SET HUB_ID = {}, LAST_UPDATE = {} WHERE REQUEST_ID = {}",
            self.table_name(),
            format_sql_value(&hub_id),
            last_update,
            format_sql_value(&request_id),
        );
        conn.execute(&query, (), None)?;
        Ok(1)
    }

    /// Update HUB_ID by TRANSPORT_TRANS_ID (từ bản tin đồng bộ CHECKIN_HUB_INFO HUB → BOO, BECT).
    /// Use ticket_in_id from Kafka message to match TRANSPORT_TRANS_ID in DB.
    pub fn update_hub_id_by_transport_trans_id(
        &self,
        transport_trans_id: i64,
        hub_id: i64,
    ) -> Result<u64, DbError> {
        use crate::db::format_sql_value;
        use crate::db::repository::{format_sql_nullable_datetime, now_utc_for_db};
        let conn = self
            .pool
            .get()
            .map_err(|e| DbError::PoolError(e.to_string()))?;
        let now = now_utc_for_db();
        let last_update = format_sql_nullable_datetime(&Some(now));
        let query = format!(
            "UPDATE {} SET HUB_ID = {}, LAST_UPDATE = {} WHERE TRANSPORT_TRANS_ID = {}",
            self.table_name(),
            format_sql_value(&hub_id),
            last_update,
            format_sql_value(&transport_trans_id),
        );
        conn.execute(&query, (), None)?;
        Ok(1)
    }

    /// Cập nhật các trường cần thiết cho checkin commit (BECT).
    /// Chỉ các trường liên quan; không ghi đè datetime khác (vd. CHECKIN_DATETIME).
    pub fn update_checkin_commit(
        &self,
        transport_trans_id: i64,
        checkin_commit_datetime: &str,
        checkin_img_count: i32,
        last_update: &str,
        checkin_pass: &str,
        fe_weight: Option<i64>,
        fe_reason_id: Option<i64>,
        img_count: Option<i32>,
        checkin_status: Option<&str>,
    ) -> Result<bool, DbError> {
        let pool = &self.pool;
        let conn =
            get_connection_with_retry(pool).map_err(|e| DbError::PoolError(e.to_string()))?;

        // Dựng SET clauses động (format_sql_datetime_str chuẩn hóa, tránh mất phần giờ). Không ghi đè BOO_ETAG (đã lưu lúc insert, thống nhất BOO).
        let mut set_parts = vec![
            format!(
                "CHECKIN_COMMIT_DATETIME = {}",
                format_sql_datetime_str(checkin_commit_datetime)
            ),
            format!("CHECKIN_IMG_COUNT = {}", checkin_img_count),
            format!("LAST_UPDATE = {}", format_sql_datetime_str(last_update)),
            format!("CHECKIN_PASS = '{}'", checkin_pass),
        ];

        // checkin_status per Java CheckioStatus: "1" = CHECKIN (chờ commit), "2" = PASS (đã commit). Mặc định "2" nếu không truyền.
        if let Some(cs) = checkin_status {
            set_parts.push(format!("CHECKIN_STATUS = '{}'", cs));
        } else {
            set_parts.push(format!("CHECKIN_STATUS = '2'"));
        }
        if let Some(w) = fe_weight {
            set_parts.push(format!("FE_WEIGHT = {}", w));
        }

        if let Some(r) = fe_reason_id {
            set_parts.push(format!("FE_REASON_ID = {}", r));
        }

        if let Some(ic) = img_count {
            set_parts.push(format!("IMG_COUNT = {}", ic));
        }

        // Chỉ cập nhật bản ghi đang chờ commit checkin (CHECKIN_STATUS = '1' per Java CheckioStatus). Sau khi commit set CHECKIN_STATUS = '2' (PASS).
        let sql = format!(
            "UPDATE RATING_OWNER.TRANSPORT_TRANSACTION_STAGE SET {} WHERE TRANSPORT_TRANS_ID = {} AND CHECKIN_STATUS = '1'",
            set_parts.join(", "),
            transport_trans_id
        );

        tracing::debug!(
            transport_trans_id,
            sql_len = sql.len(),
            "[DB] update_checkin_commit"
        );
        // Oracle (và nhiều driver ODBC) không trả cursor cho UPDATE → execute() trả None; coi là thành công nếu không lỗi.
        conn.execute(&sql, (), None)?;
        Ok(true)
    }

    /// Get record theo etag latest by checkIn time (whether checkout or not).
    /// Used when checkout needs latest record to return error if already checkout.
    pub fn find_latest_by_etag(
        &self,
        etag: &str,
    ) -> Result<Option<TransportTransactionStage>, DbError> {
        let pool = &self.pool;
        let conn =
            get_connection_with_retry(pool).map_err(|e| DbError::PoolError(e.to_string()))?;

        let etag_escaped = etag.replace("'", "''");
        let query = format!(
            "SELECT {} FROM {} WHERE ETAG_NUMBER = '{}' AND CHECKIN_STATUS IN ('1', '2') ORDER BY NVL(CHECKIN_DATETIME, INSERT_DATETIME) DESC",
            TRANSPORT_TRANSACTION_STAGE_SELECT,
            self.table_name(),
            etag_escaped
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

    /// Get record theo TICKET_IN_ID (regardless of checkout state). Used for idempotency insert: avoid double when record with same ticket_in_id exists.
    pub fn find_by_ticket_in_id(
        &self,
        ticket_in_id: i64,
    ) -> Result<Option<TransportTransactionStage>, DbError> {
        use crate::db::format_sql_value;
        let pool = &self.pool;
        let conn =
            get_connection_with_retry(pool).map_err(|e| DbError::PoolError(e.to_string()))?;
        let query = format!(
            "SELECT {} FROM {} WHERE TICKET_IN_ID = {} AND CHECKIN_STATUS IN ('1', '2') ORDER BY NVL(CHECKIN_DATETIME, INSERT_DATETIME) DESC",
            TRANSPORT_TRANSACTION_STAGE_SELECT,
            self.table_name(),
            format_sql_value(&ticket_in_id)
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

    /// Cập nhật các trường checkout cho BECT (chưa commit).
    /// Chỉ các trường liên quan checkout; không ghi đè datetime khác.
    /// account_trans_id: ID ACCOUNT_TRANSACTION tạo lúc checkout. checkout_plate_status: "1" pass, "0" fail. status: DB 1 = success, 0 = error.
    pub fn update_checkout_info(
        &self,
        transport_trans_id: i64,
        checkout_toll_id: i64,
        checkout_lane_id: i64,
        checkout_plate: &str,
        total_amount: i64,
        checkout_datetime: &str,
        last_update: &str,
        charge_status: &str,
        charge_type: &str,
        vehicle_type: &str,
        turning_code: Option<&str>,
        checkout_tid: Option<&str>,
        charge_in: &str,
        account_id: i64,
        charge_in_status: Option<&str>,
        checkout_status: Option<&str>,
        account_trans_id: Option<i64>,
        checkout_plate_status: Option<&str>,
        checkout_channel: Option<i64>,
        checkout_pass: Option<&str>,
        checkout_pass_reason_id: Option<&str>,
        checkout_img_count: Option<i32>,
        status: Option<&str>,
    ) -> Result<bool, DbError> {
        let pool = &self.pool;
        let conn =
            get_connection_with_retry(pool).map_err(|e| DbError::PoolError(e.to_string()))?;

        // Không ghi đè BOO_ETAG: giá trị đúng đã lưu lúc insert (etag number), thống nhất với BOO_TRANSPORT_TRANS_STAGE
        let mut set_parts = vec![
            format!("CHECKOUT_TOLL_ID = {}", checkout_toll_id),
            format!("CHECKOUT_LANE_ID = {}", checkout_lane_id),
            format!("CHECKOUT_PLATE = '{}'", checkout_plate.replace("'", "''")),
            format!("PLATE = '{}'", checkout_plate.replace("'", "''")), // Chỉ gán khi checkout, đồng bộ với checkout_plate
            format!("TOTAL_AMOUNT = {}", total_amount),
            format!(
                "CHECKOUT_DATETIME = {}",
                format_sql_datetime_str(checkout_datetime)
            ),
            format!("LAST_UPDATE = {}", format_sql_datetime_str(last_update)),
            format!("CHARGE_STATUS = '{}'", charge_status),
            format!("CHARGE_TYPE = '{}'", charge_type),
            format!("VEHICLE_TYPE = '{}'", vehicle_type.replace("'", "''")),
            format!("CHARGE_IN = '{}'", charge_in),
            format!("ACCOUNT_ID = {}", account_id),
            format!("TICKET_ETAG_ID = {}", transport_trans_id),
            format!("TICKET_OUT_ID = {}", transport_trans_id),
        ];

        if let Some(cs) = checkout_status {
            set_parts.push(format!("CHECKOUT_STATUS = '{}'", cs));
        }
        if let Some(cps) = checkout_plate_status {
            set_parts.push(format!(
                "CHECKOUT_PLATE_STATUS = '{}'",
                cps.replace("'", "''")
            ));
        }
        if let Some(cc) = checkout_channel {
            set_parts.push(format!("CHECKOUT_CHANNEL = {}", cc));
        }
        if let Some(cp) = checkout_pass {
            set_parts.push(format!("CHECKOUT_PASS = '{}'", cp.replace("'", "''")));
        }
        if let Some(cpr) = checkout_pass_reason_id {
            set_parts.push(format!(
                "CHECKOUT_PASS_REASON_ID = '{}'",
                cpr.replace("'", "''")
            ));
        }
        if let Some(cic) = checkout_img_count {
            set_parts.push(format!("CHECKOUT_IMG_COUNT = {}", cic));
        }

        if let Some(tc) = turning_code {
            set_parts.push(format!("TURNING_CODE = '{}'", tc.replace("'", "''")));
        }

        if let Some(ctid) = checkout_tid {
            set_parts.push(format!("CHECKOUT_TID = '{}'", ctid.replace("'", "''")));
        }

        if let Some(cis) = charge_in_status {
            set_parts.push(format!("CHARGE_IN_STATUS = '{}'", cis.replace("'", "''")));
        }

        if let Some(atid) = account_trans_id {
            if atid > 0 {
                set_parts.push(format!("ACCOUNT_TRANS_ID = {}", atid));
            }
        }

        // DB: STATUS 1 = success, 0 = error
        if let Some(s) = status {
            set_parts.push(format!("STATUS = '{}'", s.replace("'", "''")));
        }

        let sql = format!(
            "UPDATE {} SET {} WHERE {} = {}",
            self.table_name(),
            set_parts.join(", "),
            self.primary_key(),
            transport_trans_id
        );

        tracing::debug!(
            transport_trans_id,
            sql_len = sql.len(),
            "[DB] update_checkout_info"
        );
        // Oracle (và nhiều driver ODBC) không trả cursor cho UPDATE → execute() trả None; coi là thành công nếu không lỗi.
        conn.execute(&sql, (), None)?;
        Ok(true)
    }
}

impl Repository<TransportTransactionStage, i64> for TransportTransactionStageRepository {
    fn get_pool(&self) -> &Pool<OdbcConnectionManager> {
        &self.pool
    }

    fn table_name(&self) -> &str {
        "RATING_OWNER.TRANSPORT_TRANSACTION_STAGE"
    }

    fn primary_key(&self) -> &str {
        "TRANSPORT_TRANS_ID"
    }

    fn mapper(&self) -> &dyn RowMapper<TransportTransactionStage> {
        &self.mapper
    }

    /// Override find_by_id: dùng TO_CHAR for datetime columns để giữ phần giờ khi đọc.
    fn find_by_id(&self, id: i64) -> Result<Option<TransportTransactionStage>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!(
            "SELECT {} FROM {} WHERE {} = {}",
            TRANSPORT_TRANSACTION_STAGE_SELECT,
            self.table_name(),
            self.primary_key(),
            id
        );
        let mapper = self.mapper();
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

    /// Override find_all: dùng TO_CHAR for datetime columns để giữ phần giờ khi đọc.
    fn find_all(&self) -> Result<Vec<TransportTransactionStage>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!(
            "SELECT {} FROM {}",
            TRANSPORT_TRANSACTION_STAGE_SELECT,
            self.table_name()
        );
        let mapper = self.mapper();
        let cursor_result = conn.execute(&query, (), None)?;
        match cursor_result {
            Some(mut cursor) => {
                let mut results = Vec::new();
                loop {
                    match cursor.next_row() {
                        Ok(Some(mut row)) => match mapper.map_row(&mut row) {
                            Ok(entity) => results.push(entity),
                            Err(e) => {
                                tracing::error!(error = %e, "[DB] TRANSPORT_TRANSACTION_STAGE map_row failed");
                                continue;
                            }
                        },
                        Ok(None) => break,
                        Err(e) => {
                            return Err(DbError::ExecutionError(format!(
                                "Error fetching row: {}",
                                e
                            )))
                        }
                    }
                }
                Ok(results)
            }
            None => Ok(Vec::new()),
        }
    }

    fn build_insert_query(&self, entity: &TransportTransactionStage) -> Result<String, DbError> {
        use crate::db::{
            format_sql_nullable_f64, format_sql_nullable_i32, format_sql_nullable_i64,
            format_sql_nullable_string, format_sql_string, format_sql_value,
        };

        // Dùng format_sql_nullable_datetime để chuẩn hóa (YYYY-MM-DD HH24:MI:SS), tránh mất phần giờ khi lưu
        let checkin_commit_datetime = format_sql_nullable_datetime(&entity.checkin_commit_datetime);
        let checkout_commit_datetime =
            format_sql_nullable_datetime(&entity.checkout_commit_datetime);
        let checkin_datetime = format_sql_nullable_datetime(&entity.checkin_datetime);
        let checkout_datetime = format_sql_nullable_datetime(&entity.checkout_datetime);
        let last_update = format_sql_nullable_datetime(&entity.last_update);
        let insert_datetime = match &entity.insert_datetime {
            Some(_) => format_sql_nullable_datetime(&entity.insert_datetime),
            None => "SYSDATE".to_string(),
        };
        let charge_datetime = format_sql_nullable_datetime(&entity.charge_datetime);

        Ok(format!(
            "INSERT INTO {} (TRANSPORT_TRANS_ID, SUBSCRIBER_ID, ETAG_ID, VEHICLE_ID, CHECKIN_TOLL_ID, CHECKIN_LANE_ID, CHECKIN_COMMIT_DATETIME, CHECKIN_CHANNEL, CHECKIN_PASS, CHECKIN_PASS_REASON_ID, CHECKOUT_TOLL_ID, CHECKOUT_LANE_ID, CHECKOUT_COMMIT_DATETIME, CHECKOUT_CHANNEL, CHECKOUT_PASS, CHECKOUT_PASS_REASON_ID, CHARGE_STATUS, CHARGE_TYPE, TOTAL_AMOUNT, ACCOUNT_ID, ACCOUNT_TRANS_ID, CHECKIN_DATETIME, CHECKOUT_DATETIME, CHECKIN_STATUS, ETAG_NUMBER, REQUEST_ID, CHECKIN_PLATE, CHECKIN_PLATE_STATUS, CHECKOUT_PLATE, CHECKOUT_PLATE_STATUS, PLATE_FROM_TOLL, IMG_COUNT, CHECKIN_IMG_COUNT, CHECKOUT_IMG_COUNT, STATUS, LAST_UPDATE, NOTIF_SCAN, TOLL_TYPE, RATING_TYPE, INSERT_DATETIME, INSUFF_AMOUNT, CHECKOUT_STATUS, PLATE, VEHICLE_TYPE, FE_VEHICLE_LENGTH, FE_COMMIT_AMOUNT, FE_WEIGHT, FE_REASON_ID, VEHICLE_TYPE_PROFILE, BOO, BOO_TRANSPORT_TRANS_ID, CHECKIN_SHIFT, CHECKOUT_SHIFT, CHECKIN_TID, CHECKOUT_TID, CHARGE_IN, CHARGE_TRANS_ID, BALANCE, CHARGE_IN_STATUS, CHARGE_DATETIME, CHARGE_IN_104, FE_ONLINE_STATUS, CHD_TYPE, CHD_REF_ID, CHD_REASON, FE_TRANS_ID, TRANSITION_CLOSE, VOUCHER_CODE, VOUCHER_USED_AMOUNT, VOUCHER_AMOUNT, TRANSPORT_SYNC_ID, TICKET_IN_ID, HUB_ID, TICKET_ETAG_ID, TICKET_OUT_ID, BOO_ETAG) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
            self.table_name(),
            format_sql_value(&entity.transport_trans_id),
            format_sql_nullable_i64(&entity.subscriber_id),
            format_sql_nullable_i64(&entity.etag_id),
            format_sql_nullable_i64(&entity.vehicle_id),
            format_sql_nullable_i64(&entity.checkin_toll_id),
            format_sql_nullable_i64(&entity.checkin_lane_id),
            checkin_commit_datetime,
            format_sql_nullable_i64(&entity.checkin_channel),
            format_sql_nullable_string(&entity.checkin_pass),
            format_sql_nullable_string(&entity.checkin_pass_reason_id),
            format_sql_nullable_i64(&entity.checkout_toll_id),
            format_sql_nullable_i64(&entity.checkout_lane_id),
            checkout_commit_datetime,
            format_sql_nullable_i64(&entity.checkout_channel),
            format_sql_nullable_string(&entity.checkout_pass),
            format_sql_nullable_string(&entity.checkout_pass_reason_id),
            format_sql_nullable_string(&entity.charge_status),
            format_sql_nullable_string(&entity.charge_type),
            format_sql_nullable_i64(&entity.total_amount),
            format_sql_nullable_i64(&entity.account_id),
            format_sql_nullable_i64(&entity.account_trans_id),
            checkin_datetime,
            checkout_datetime,
            format_sql_nullable_string(&entity.checkin_status),
            format_sql_nullable_string(&entity.etag_number),
            format_sql_nullable_i64(&entity.request_id),
            format_sql_nullable_string(&entity.checkin_plate),
            format_sql_nullable_string(&entity.checkin_plate_status),
            format_sql_nullable_string(&entity.checkout_plate),
            format_sql_nullable_string(&entity.checkout_plate_status),
            format_sql_nullable_string(&entity.plate_from_toll),
            format_sql_nullable_i32(&entity.img_count),
            format_sql_nullable_i32(&entity.checkin_img_count),
            format_sql_nullable_i32(&entity.checkout_img_count),
            format_sql_nullable_string(&entity.status),
            last_update,
            format_sql_nullable_i32(&entity.notif_scan),
            format_sql_nullable_string(&entity.toll_type),
            format_sql_nullable_string(&entity.rating_type),
            insert_datetime,
            format_sql_nullable_f64(&entity.insuff_amount),
            format_sql_nullable_string(&entity.checkout_status),
            format_sql_nullable_string(&entity.plate),
            format_sql_nullable_string(&entity.vehicle_type),
            format_sql_nullable_i64(&entity.fe_vehicle_length),
            format_sql_nullable_i64(&entity.fe_commit_amount),
            format_sql_nullable_i64(&entity.fe_weight),
            format_sql_nullable_i64(&entity.fe_reason_id),
            format_sql_nullable_string(&entity.vehicle_type_profile),
            format_sql_nullable_i64(&entity.boo),
            format_sql_nullable_i64(&entity.boo_transport_trans_id),
            format_sql_nullable_string(&entity.checkin_shift),
            format_sql_nullable_string(&entity.checkout_shift),
            format_sql_nullable_string(&entity.checkin_tid),
            format_sql_nullable_string(&entity.checkout_tid),
            format_sql_nullable_string(&entity.charge_in),
            format_sql_nullable_string(&entity.charge_trans_id),
            format_sql_nullable_f64(&entity.balance),
            format_sql_nullable_string(&entity.charge_in_status),
            charge_datetime,
            format_sql_nullable_string(&entity.charge_in_104),
            format_sql_nullable_string(&entity.fe_online_status),
            format_sql_nullable_string(&entity.chd_type),
            format_sql_nullable_i64(&entity.chd_ref_id),
            format_sql_nullable_string(&entity.chd_reason),
            format_sql_nullable_string(&entity.fe_trans_id),
            // TRANSITION_CLOSE NOT NULL; default 'F' when None or empty (Oracle treats '' as NULL).
            format_sql_string(
                entity
                    .transition_close
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("F"),
            ),
            format_sql_nullable_string(&entity.voucher_code),
            format_sql_nullable_i64(&entity.voucher_used_amount),
            format_sql_nullable_i64(&entity.voucher_amount),
            format_sql_nullable_i64(&entity.transport_sync_id),
            format_sql_nullable_i64(&entity.ticket_in_id),
            format_sql_nullable_i64(&entity.hub_id),
            format_sql_nullable_i64(&entity.ticket_eTag_id),
            format_sql_nullable_i64(&entity.ticket_out_id),
            format_sql_nullable_string(&entity.boo_etag),
        ))
    }

    fn build_update_query(
        &self,
        id: i64,
        entity: &TransportTransactionStage,
    ) -> Result<String, DbError> {
        use crate::db::{
            format_sql_nullable_f64, format_sql_nullable_i32, format_sql_nullable_i64,
            format_sql_nullable_string, format_sql_string, format_sql_value,
        };

        let checkin_commit_datetime = format_sql_nullable_datetime(&entity.checkin_commit_datetime);
        let checkout_commit_datetime =
            format_sql_nullable_datetime(&entity.checkout_commit_datetime);
        let checkin_datetime = format_sql_nullable_datetime(&entity.checkin_datetime);
        let checkout_datetime = format_sql_nullable_datetime(&entity.checkout_datetime);
        let last_update = format_sql_nullable_datetime(&entity.last_update);
        let insert_datetime = format_sql_nullable_datetime(&entity.insert_datetime);
        let charge_datetime = format_sql_nullable_datetime(&entity.charge_datetime);

        Ok(format!(
            "UPDATE {} SET SUBSCRIBER_ID = {}, ETAG_ID = {}, VEHICLE_ID = {}, CHECKIN_TOLL_ID = {}, CHECKIN_LANE_ID = {}, CHECKIN_COMMIT_DATETIME = {}, CHECKIN_CHANNEL = {}, CHECKIN_PASS = {}, CHECKIN_PASS_REASON_ID = {}, CHECKOUT_TOLL_ID = {}, CHECKOUT_LANE_ID = {}, CHECKOUT_COMMIT_DATETIME = {}, CHECKOUT_CHANNEL = {}, CHECKOUT_PASS = {}, CHECKOUT_PASS_REASON_ID = {}, CHARGE_STATUS = {}, CHARGE_TYPE = {}, TOTAL_AMOUNT = {}, ACCOUNT_ID = {}, ACCOUNT_TRANS_ID = {}, CHECKIN_DATETIME = {}, CHECKOUT_DATETIME = {}, CHECKIN_STATUS = {}, ETAG_NUMBER = {}, REQUEST_ID = {}, CHECKIN_PLATE = {}, CHECKIN_PLATE_STATUS = {}, CHECKOUT_PLATE = {}, CHECKOUT_PLATE_STATUS = {}, PLATE_FROM_TOLL = {}, IMG_COUNT = {}, CHECKIN_IMG_COUNT = {}, CHECKOUT_IMG_COUNT = {}, STATUS = {}, LAST_UPDATE = {}, NOTIF_SCAN = {}, TOLL_TYPE = {}, RATING_TYPE = {}, INSERT_DATETIME = {}, INSUFF_AMOUNT = {}, CHECKOUT_STATUS = {}, PLATE = {}, VEHICLE_TYPE = {}, FE_VEHICLE_LENGTH = {}, FE_COMMIT_AMOUNT = {}, FE_WEIGHT = {}, FE_REASON_ID = {}, VEHICLE_TYPE_PROFILE = {}, BOO = {}, BOO_TRANSPORT_TRANS_ID = {}, CHECKIN_SHIFT = {}, CHECKOUT_SHIFT = {}, CHECKIN_TID = {}, CHECKOUT_TID = {}, CHARGE_IN = {}, CHARGE_TRANS_ID = {}, BALANCE = {}, CHARGE_IN_STATUS = {}, CHARGE_DATETIME = {}, CHARGE_IN_104 = {}, FE_ONLINE_STATUS = {}, CHD_TYPE = {}, CHD_REF_ID = {}, CHD_REASON = {}, FE_TRANS_ID = {}, TRANSITION_CLOSE = {}, VOUCHER_CODE = {}, VOUCHER_USED_AMOUNT = {}, VOUCHER_AMOUNT = {}, TRANSPORT_SYNC_ID = {}, TICKET_IN_ID = {}, HUB_ID = {}, TICKET_ETAG_ID = {}, TICKET_OUT_ID = {}, BOO_ETAG = {} WHERE {} = {}",
            self.table_name(),
            format_sql_nullable_i64(&entity.subscriber_id),
            format_sql_nullable_i64(&entity.etag_id),
            format_sql_nullable_i64(&entity.vehicle_id),
            format_sql_nullable_i64(&entity.checkin_toll_id),
            format_sql_nullable_i64(&entity.checkin_lane_id),
            checkin_commit_datetime,
            format_sql_nullable_i64(&entity.checkin_channel),
            format_sql_nullable_string(&entity.checkin_pass),
            format_sql_nullable_string(&entity.checkin_pass_reason_id),
            format_sql_nullable_i64(&entity.checkout_toll_id),
            format_sql_nullable_i64(&entity.checkout_lane_id),
            checkout_commit_datetime,
            format_sql_nullable_i64(&entity.checkout_channel),
            format_sql_nullable_string(&entity.checkout_pass),
            format_sql_nullable_string(&entity.checkout_pass_reason_id),
            format_sql_nullable_string(&entity.charge_status),
            format_sql_nullable_string(&entity.charge_type),
            format_sql_nullable_i64(&entity.total_amount),
            format_sql_nullable_i64(&entity.account_id),
            format_sql_nullable_i64(&entity.account_trans_id),
            checkin_datetime,
            checkout_datetime,
            format_sql_nullable_string(&entity.checkin_status),
            format_sql_nullable_string(&entity.etag_number),
            format_sql_nullable_i64(&entity.request_id),
            format_sql_nullable_string(&entity.checkin_plate),
            format_sql_nullable_string(&entity.checkin_plate_status),
            format_sql_nullable_string(&entity.checkout_plate),
            format_sql_nullable_string(&entity.checkout_plate_status),
            format_sql_nullable_string(&entity.plate_from_toll),
            format_sql_nullable_i32(&entity.img_count),
            format_sql_nullable_i32(&entity.checkin_img_count),
            format_sql_nullable_i32(&entity.checkout_img_count),
            format_sql_nullable_string(&entity.status),
            last_update,
            format_sql_nullable_i32(&entity.notif_scan),
            format_sql_nullable_string(&entity.toll_type),
            format_sql_nullable_string(&entity.rating_type),
            insert_datetime,
            format_sql_nullable_f64(&entity.insuff_amount),
            format_sql_nullable_string(&entity.checkout_status),
            format_sql_nullable_string(&entity.plate),
            format_sql_nullable_string(&entity.vehicle_type),
            format_sql_nullable_i64(&entity.fe_vehicle_length),
            format_sql_nullable_i64(&entity.fe_commit_amount),
            format_sql_nullable_i64(&entity.fe_weight),
            format_sql_nullable_i64(&entity.fe_reason_id),
            // Default to 'STD' if None to prevent "NULL into NOT NULL" error
            if let Some(ref v) = entity.vehicle_type_profile {
                format!("'{}'", v)
            } else {
                "'STD'".to_string()
            },
            format_sql_nullable_i64(&entity.boo),
            format_sql_nullable_i64(&entity.boo_transport_trans_id),
            format_sql_nullable_string(&entity.checkin_shift.as_ref().map(|s| s.chars().take(10).collect::<String>())),
            format_sql_nullable_string(&entity.checkout_shift.as_ref().map(|s| s.chars().take(10).collect::<String>())),
            format_sql_nullable_string(&entity.checkin_tid),
            format_sql_nullable_string(&entity.checkout_tid),
            format_sql_nullable_string(&entity.charge_in),
            format_sql_nullable_string(&entity.charge_trans_id),
            format_sql_nullable_f64(&entity.balance),
            format_sql_nullable_string(&entity.charge_in_status),
            charge_datetime,
            format_sql_nullable_string(&entity.charge_in_104),
            format_sql_nullable_string(&entity.fe_online_status),
            format_sql_nullable_string(&entity.chd_type),
            format_sql_nullable_i64(&entity.chd_ref_id),
            format_sql_nullable_string(&entity.chd_reason),
            format_sql_nullable_string(&entity.fe_trans_id),
            // TRANSITION_CLOSE NOT NULL; default 'F' when None or empty (Oracle '' = NULL).
            if let Some(ref v) = entity.transition_close.as_ref().filter(|s| !s.is_empty()) {
                format_sql_string(v.as_str())
            } else {
                format_sql_string("F")
            },
            format_sql_nullable_string(&entity.voucher_code),
            format_sql_nullable_i64(&entity.voucher_used_amount),
            format_sql_nullable_i64(&entity.voucher_amount),
            format_sql_nullable_i64(&entity.transport_sync_id),
            format_sql_nullable_i64(&entity.ticket_in_id),
            format_sql_nullable_i64(&entity.hub_id),
            format_sql_nullable_i64(&entity.ticket_eTag_id),
            format_sql_nullable_i64(&entity.ticket_out_id),
            format_sql_nullable_string(&entity.boo_etag),
            self.primary_key(),
            format_sql_value(&id),
        ))
    }

    fn get_last_insert_id(
        &self,
        _conn: &mut odbc_api::Connection<'static>,
    ) -> Result<i64, DbError> {
        // Use sequence to get next ID
        use crate::db::sequence::get_next_sequence_value_with_schema;
        get_next_sequence_value_with_schema(
            self.get_pool(),
            "RATING_OWNER",
            "TRANSPORT_TRANS_STAGE_SEQ",
        )
    }

    /// Override insert để tự động lấy ID từ sequence.
    /// Idempotency: (1) theo ticket_in_id — pending record with same TICKET_IN_ID then skip insert;
    /// (2) theo transport_trans_id — record with same ID exists then skip insert.
    fn insert(&self, entity: &TransportTransactionStage) -> Result<i64, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        let mut entity_to_insert = entity.clone();

        // Idempotency theo ticket_in_id (regardless of checkout state): avoid double save when record with same TICKET_IN_ID exists.
        if let Some(tid) = entity_to_insert.ticket_in_id {
            if tid != 0 {
                match self.find_by_ticket_in_id(tid) {
                    Ok(Some(existing)) => {
                        tracing::info!(
                            transport_trans_id = existing.transport_trans_id,
                            ticket_in_id = tid,
                            request_id = ?entity_to_insert.request_id,
                            "[DB] TRANSPORT_TRANSACTION_STAGE already exists for TICKET_IN_ID (idempotency), skipping insert"
                        );
                        return Ok(existing.transport_trans_id);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(
                            ticket_in_id = tid,
                            error = %e,
                            "[DB] TRANSPORT_TRANSACTION_STAGE find_by_ticket_in_id failed, will attempt insert"
                        );
                    }
                }
            }
        }

        // Nếu transport_trans_id = 0, lấy từ sequence. Nếu đã được set (khác 0), dùng giá trị đó
        if entity_to_insert.transport_trans_id == 0 {
            use crate::db::sequence::get_next_sequence_value_with_schema;
            entity_to_insert.transport_trans_id = get_next_sequence_value_with_schema(
                pool,
                "RATING_OWNER",
                "TRANSPORT_TRANS_STAGE_SEQ",
            )?;
        }

        // Idempotency check: If transport_trans_id is set (non-zero), check if record already exists
        // Tránh duplicate record khi retry hoặc race condition
        if entity_to_insert.transport_trans_id != 0 {
            match self.find_by_id(entity_to_insert.transport_trans_id) {
                Ok(Some(_existing_record)) => {
                    tracing::info!(
                        transport_trans_id = entity_to_insert.transport_trans_id,
                        request_id = ?entity_to_insert.request_id,
                        "[DB] TRANSPORT_TRANSACTION_STAGE already exists (idempotency check), skipping insert"
                    );
                    // Record already exists, return success without re-insert
                    return Ok(entity_to_insert.transport_trans_id);
                }
                Ok(None) => {
                    // Record does not exist, continue with insert
                }
                Err(e) => {
                    // If error during check, log warning but still continue with insert
                    // (may be DB connection issue, should not block insert)
                    tracing::warn!(
                        transport_trans_id = entity_to_insert.transport_trans_id,
                        error = %e,
                        "[DB] TRANSPORT_TRANSACTION_STAGE idempotency check failed, will attempt insert"
                    );
                }
            }
        }

        let query = self.build_insert_query(&entity_to_insert)?;

        tracing::debug!(
            transport_trans_id = entity_to_insert.transport_trans_id,
            request_id = ?entity_to_insert.request_id,
            ticket_in_id = ?entity_to_insert.ticket_in_id,
            "[DB] INSERT TRANSPORT_TRANSACTION_STAGE"
        );
        let cursor_result = match conn.execute(&query, (), None) {
            Ok(result) => result,
            Err(e) => {
                tracing::error!(
                    transport_trans_id = entity_to_insert.transport_trans_id,
                    request_id = ?entity_to_insert.request_id,
                    error = %e,
                    "[DB] INSERT TRANSPORT_TRANSACTION_STAGE failed"
                );
                tracing::debug!(entity = ?entity_to_insert, "[DB] INSERT failed entity");
                return Err(DbError::ExecutionError(format!(
                    "Failed to insert TRANSPORT_TRANSACTION_STAGE: {}",
                    e
                )));
            }
        };

        // If cursor exists, close it
        if let Some(_cursor) = cursor_result {
            // Cursor is dropped automatically
        }

        // Return the set transport_trans_id
        Ok(entity_to_insert.transport_trans_id)
    }
}

impl TransportTransactionStageRepository {
    /// Get next ticket_id from sequence (TRANSPORT_TRANS_STAGE_SEQ).
    /// Dùng cho BECT checkin trước khi tạo TRANSPORT_TRANSACTION_STAGE.
    pub fn get_next_ticket_id(&self) -> Result<i64, DbError> {
        use crate::db::sequence::get_next_sequence_value_with_schema;
        get_next_sequence_value_with_schema(
            self.get_pool(),
            "RATING_OWNER",
            "TRANSPORT_TRANS_STAGE_SEQ",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requires RATING_DB_* env and live DB. Run: cargo test -- --ignored
    #[test]
    #[ignore = "requires RATING_DB_* env and live DB"]
    fn test_transport_transaction_stage_repository() {
        let repo = TransportTransactionStageRepository::new();
        assert_eq!(
            repo.table_name(),
            "RATING_OWNER.TRANSPORT_TRANSACTION_STAGE"
        );
        assert_eq!(repo.primary_key(), "TRANSPORT_TRANS_ID");
    }
}
