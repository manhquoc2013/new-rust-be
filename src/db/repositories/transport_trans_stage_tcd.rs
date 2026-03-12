//! Entity, mapper, repository for table TRANSPORT_TRANS_STAGE_TCD.

use std::sync::Arc;

use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::configs::rating_db::RATING_DB;
use crate::db::repository::format_sql_nullable_datetime;
use crate::db::{
    get_i64, get_nullable_datetime_string, get_nullable_i64, get_nullable_string, DbError,
    Repository, RowMapper,
};
use odbc_api::{Cursor, CursorRow};
use r2d2::Pool;
use serde::{Deserialize, Serialize};

/// Entity for table TRANSPORT_TRANS_STAGE_TCD
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportTransStageTcd {
    pub transport_stage_tcd_id: i64, // TRANSPORT_STAGE_TCD_ID - Primary Key
    pub transport_trans_id: i64,     // TRANSPORT_TRANS_ID - NOT NULL
    pub boo: i64,                    // BOO - NOT NULL
    pub bot_id: Option<i64>,         // BOT_ID
    pub toll_a_id: Option<i64>,      // TOLL_A_ID
    pub toll_b_id: Option<i64>,      // TOLL_B_ID
    pub stage_id: Option<i64>,       // STAGE_ID
    pub price_id: Option<i64>,       // PRICE_ID
    pub ticket_type: Option<String>, // TICKET_TYPE
    pub price_ticket_type: Option<String>, // PRICE_TICKET_TYPE
    pub price_amount: Option<i64>,   // PRICE_AMOUNT
    pub subscription_id: Option<String>, // SUBSCRIPTION_ID
    pub vehicle_type: Option<i64>,   // VEHICLE_TYPE
    pub mdh_id: Option<i64>,         // MDH_ID
    pub checkin_datetime: Option<String>, // CHECKIN_DATETIME
}

/// Mapper cho TRANSPORT_TRANS_STAGE_TCD
pub struct TransportTransStageTcdMapper;

impl RowMapper<TransportTransStageTcd> for TransportTransStageTcdMapper {
    fn map_row(&self, row: &mut CursorRow) -> Result<TransportTransStageTcd, DbError> {
        Ok(TransportTransStageTcd {
            transport_stage_tcd_id: get_i64(row, 1)?, // TRANSPORT_STAGE_TCD_ID
            transport_trans_id: get_i64(row, 2)?,     // TRANSPORT_TRANS_ID
            boo: get_i64(row, 3)?,                    // BOO
            bot_id: get_nullable_i64(row, 4)?,        // BOT_ID
            toll_a_id: get_nullable_i64(row, 5)?,     // TOLL_A_ID
            toll_b_id: get_nullable_i64(row, 6)?,     // TOLL_B_ID
            stage_id: get_nullable_i64(row, 7)?,      // STAGE_ID
            price_id: get_nullable_i64(row, 8)?,      // PRICE_ID
            ticket_type: get_nullable_string(row, 9)?, // TICKET_TYPE
            price_ticket_type: get_nullable_string(row, 10)?, // PRICE_TICKET_TYPE
            price_amount: get_nullable_i64(row, 11)?, // PRICE_AMOUNT
            subscription_id: get_nullable_string(row, 12)?, // SUBSCRIPTION_ID
            vehicle_type: get_nullable_i64(row, 13)?, // VEHICLE_TYPE
            mdh_id: get_nullable_i64(row, 14)?,       // MDH_ID
            checkin_datetime: get_nullable_datetime_string(row, 15)?, // CHECKIN_DATETIME
        })
    }
}

/// SELECT list with TO_CHAR for datetime columns so when reading via ODBC the time part is preserved.
const TRANSPORT_TRANS_STAGE_TCD_SELECT: &str = "TRANSPORT_STAGE_TCD_ID, TRANSPORT_TRANS_ID, BOO, BOT_ID, TOLL_A_ID, TOLL_B_ID, STAGE_ID, PRICE_ID, TICKET_TYPE, PRICE_TICKET_TYPE, PRICE_AMOUNT, SUBSCRIPTION_ID, VEHICLE_TYPE, MDH_ID, TO_CHAR(CHECKIN_DATETIME, 'YYYY-MM-DD HH24:MI:SS')";

/// Repository cho TRANSPORT_TRANS_STAGE_TCD
pub struct TransportTransStageTcdRepository {
    pool: Arc<Pool<OdbcConnectionManager>>,
    mapper: TransportTransStageTcdMapper,
}

impl TransportTransStageTcdRepository {
    pub fn new() -> Self {
        Self {
            pool: RATING_DB.clone(),
            mapper: TransportTransStageTcdMapper,
        }
    }

    /// Delete all TCD records by TRANSPORT_TRANS_ID before saving (clear then insert).
    pub fn delete_by_transport_trans_id(&self, transport_trans_id: i64) -> Result<(), DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!(
            "DELETE FROM {} WHERE TRANSPORT_TRANS_ID = {}",
            self.table_name(),
            transport_trans_id
        );
        let _cursor = conn.execute(&query, (), None)?;
        Ok(())
    }

    /// Get all rating_detail by TRANSPORT_TRANS_ID from DB
    pub fn find_by_transport_trans_id(
        &self,
        transport_trans_id: i64,
    ) -> Result<Vec<TransportTransStageTcd>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!(
            "SELECT {} FROM {} WHERE TRANSPORT_TRANS_ID = {} ORDER BY TRANSPORT_STAGE_TCD_ID",
            TRANSPORT_TRANS_STAGE_TCD_SELECT,
            self.table_name(),
            transport_trans_id
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
                                tracing::error!(error = %e, "[DB] TransportTransStageTcd map_row failed");
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
}

impl Repository<TransportTransStageTcd, i64> for TransportTransStageTcdRepository {
    fn get_pool(&self) -> &Pool<OdbcConnectionManager> {
        &self.pool
    }

    fn table_name(&self) -> &str {
        "RATING_OWNER.TRANSPORT_TRANS_STAGE_TCD"
    }

    fn primary_key(&self) -> &str {
        "TRANSPORT_STAGE_TCD_ID"
    }

    fn mapper(&self) -> &dyn RowMapper<TransportTransStageTcd> {
        &self.mapper
    }

    fn find_by_id(&self, id: i64) -> Result<Option<TransportTransStageTcd>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!(
            "SELECT {} FROM {} WHERE {} = {}",
            TRANSPORT_TRANS_STAGE_TCD_SELECT,
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

    fn find_all(&self) -> Result<Vec<TransportTransStageTcd>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!(
            "SELECT {} FROM {}",
            TRANSPORT_TRANS_STAGE_TCD_SELECT,
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
                                tracing::error!(error = %e, "[DB] map_row failed");
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

    fn build_insert_query(&self, entity: &TransportTransStageTcd) -> Result<String, DbError> {
        use crate::db::{format_sql_nullable_i64, format_sql_nullable_string, format_sql_value};

        let checkin_datetime = format_sql_nullable_datetime(&entity.checkin_datetime);

        Ok(format!(
            "INSERT INTO {} (TRANSPORT_STAGE_TCD_ID, TRANSPORT_TRANS_ID, BOO, BOT_ID, TOLL_A_ID, TOLL_B_ID, STAGE_ID, PRICE_ID, TICKET_TYPE, PRICE_TICKET_TYPE, PRICE_AMOUNT, SUBSCRIPTION_ID, VEHICLE_TYPE, MDH_ID, CHECKIN_DATETIME) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
            self.table_name(),
            format_sql_value(&entity.transport_stage_tcd_id),
            format_sql_value(&entity.transport_trans_id),
            format_sql_value(&entity.boo),
            format_sql_nullable_i64(&entity.bot_id),
            format_sql_nullable_i64(&entity.toll_a_id),
            format_sql_nullable_i64(&entity.toll_b_id),
            format_sql_nullable_i64(&entity.stage_id),
            format_sql_nullable_i64(&entity.price_id),
            format_sql_nullable_string(&entity.ticket_type),
            format_sql_nullable_string(&entity.price_ticket_type),
            format_sql_nullable_i64(&entity.price_amount),
            format_sql_nullable_string(&entity.subscription_id),
            format_sql_nullable_i64(&entity.vehicle_type),
            format_sql_nullable_i64(&entity.mdh_id),
            checkin_datetime,
        ))
    }

    fn build_update_query(
        &self,
        id: i64,
        entity: &TransportTransStageTcd,
    ) -> Result<String, DbError> {
        use crate::db::{format_sql_nullable_i64, format_sql_nullable_string, format_sql_value};

        let checkin_datetime = format_sql_nullable_datetime(&entity.checkin_datetime);

        Ok(format!(
            "UPDATE {} SET TRANSPORT_TRANS_ID = {}, BOO = {}, BOT_ID = {}, TOLL_A_ID = {}, TOLL_B_ID = {}, STAGE_ID = {}, PRICE_ID = {}, TICKET_TYPE = {}, PRICE_TICKET_TYPE = {}, PRICE_AMOUNT = {}, SUBSCRIPTION_ID = {}, VEHICLE_TYPE = {}, MDH_ID = {}, CHECKIN_DATETIME = {} WHERE {} = {}",
            self.table_name(),
            format_sql_value(&entity.transport_trans_id),
            format_sql_value(&entity.boo),
            format_sql_nullable_i64(&entity.bot_id),
            format_sql_nullable_i64(&entity.toll_a_id),
            format_sql_nullable_i64(&entity.toll_b_id),
            format_sql_nullable_i64(&entity.stage_id),
            format_sql_nullable_i64(&entity.price_id),
            format_sql_nullable_string(&entity.ticket_type),
            format_sql_nullable_string(&entity.price_ticket_type),
            format_sql_nullable_i64(&entity.price_amount),
            format_sql_nullable_string(&entity.subscription_id),
            format_sql_nullable_i64(&entity.vehicle_type),
            format_sql_nullable_i64(&entity.mdh_id),
            checkin_datetime,
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
            "TRANSPORT_STAGE_TCD_HIS_SEQ",
        )
    }

    /// Override insert to auto-fetch ID from sequence if not set
    fn insert(&self, entity: &TransportTransStageTcd) -> Result<i64, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        // Nếu transport_stage_tcd_id chưa được set (0), lấy từ sequence
        let mut entity_to_insert = entity.clone();
        if entity_to_insert.transport_stage_tcd_id == 0 {
            use crate::db::sequence::get_next_sequence_value_with_schema;
            entity_to_insert.transport_stage_tcd_id = get_next_sequence_value_with_schema(
                pool,
                "RATING_OWNER",
                "TRANSPORT_STAGE_TCD_HIS_SEQ",
            )?;
        }

        let query = self.build_insert_query(&entity_to_insert)?;

        // Execute INSERT query
        let cursor_result = conn.execute(&query, (), None)?;

        // If cursor exists, close it
        if let Some(_cursor) = cursor_result {
            // Cursor is dropped automatically
        }

        // Trả về transport_stage_tcd_id đã được set
        Ok(entity_to_insert.transport_stage_tcd_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requires RATING_DB_* env and live DB. Run: cargo test -- --ignored
    #[test]
    #[ignore = "requires RATING_DB_* env and live DB"]
    fn test_transport_trans_stage_tcd_repository() {
        let repo = TransportTransStageTcdRepository::new();
        assert_eq!(repo.table_name(), "RATING_OWNER.TRANSPORT_TRANS_STAGE_TCD");
        assert_eq!(repo.primary_key(), "TRANSPORT_STAGE_TCD_ID");
    }
}
