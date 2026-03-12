//! Entity, mapper, repository for table TCOC_REQUEST.

use crate::configs::mediation_db::MEDIATION_DB;
use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::repository::format_sql_nullable_datetime;
use crate::db::{
    get_nullable_datetime_string, get_nullable_i64, get_nullable_string, DbError, Repository,
    RowMapper,
};
use crate::models::TCOCmessages::FE_REQUEST;
use chrono::Utc;
use odbc_api::{Cursor, CursorRow};
use r2d2::Pool;
use std::error::Error;

/// Entity for table TCOC_REQUEST
#[derive(Debug, Clone)]
pub struct TcocRequest {
    pub id: Option<i64>,
    pub request_id: Option<i64>,
    pub command_id: Option<i64>,
    pub session_id: Option<i64>,
    pub toll_id: Option<i64>,
    pub etag_id: Option<String>,
    pub lane_id: Option<i64>,
    pub plate: Option<String>,
    pub content: Option<String>,
    pub description: Option<String>,
    pub request_datetime: Option<String>,
    pub toll_in: Option<i64>,
    pub toll_out: Option<i64>,
    pub image_count: Option<i64>,
    pub tid: Option<String>,
    pub node_id: Option<String>,
}

/// Mapper cho TCOC_REQUEST
pub struct TcocRequestMapper;

impl RowMapper<TcocRequest> for TcocRequestMapper {
    fn map_row(&self, row: &mut CursorRow) -> Result<TcocRequest, DbError> {
        Ok(TcocRequest {
            id: get_nullable_i64(row, 1)?,                            // ID
            request_id: get_nullable_i64(row, 2)?,                    // REQUEST_ID
            command_id: get_nullable_i64(row, 3)?,                    // COMMAND_ID
            session_id: get_nullable_i64(row, 4)?,                    // SESSION_ID
            toll_id: get_nullable_i64(row, 5)?,                       // TOLL_ID
            etag_id: get_nullable_string(row, 6)?,                    // ETAG_ID
            lane_id: get_nullable_i64(row, 7)?,                       // LANE_ID
            plate: get_nullable_string(row, 8)?,                      // PLATE
            content: get_nullable_string(row, 9)?,                    // CONTENT
            description: get_nullable_string(row, 10)?,               // DESCRIPTION
            request_datetime: get_nullable_datetime_string(row, 11)?, // REQUEST_DATETIME
            toll_in: get_nullable_i64(row, 12)?,                      // TOLL_IN
            toll_out: get_nullable_i64(row, 13)?,                     // TOLL_OUT
            image_count: get_nullable_i64(row, 14)?,                  // IMAGE_COUNT
            tid: get_nullable_string(row, 15)?,                       // TID
            node_id: get_nullable_string(row, 16)?,                   // NODE_ID
        })
    }
}

/// SELECT list with TO_CHAR for datetime columns so when reading via ODBC the time part is preserved.
const TCOC_REQUEST_SELECT: &str = "ID, REQUEST_ID, COMMAND_ID, SESSION_ID, TOLL_ID, ETAG_ID, LANE_ID, PLATE, CONTENT, DESCRIPTION, TO_CHAR(REQUEST_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), TOLL_IN, TOLL_OUT, IMAGE_COUNT, TID, NODE_ID";

/// Repository cho TCOC_REQUEST
pub struct TcocRequestRepository {
    pool: Pool<OdbcConnectionManager>,
    mapper: TcocRequestMapper,
}

impl TcocRequestRepository {
    pub fn new() -> Self {
        Self {
            pool: MEDIATION_DB.clone(),
            mapper: TcocRequestMapper,
        }
    }

    /// Get record TCOC_REQUEST theo REQUEST_ID (bản ghi mới nhất nếu có nhiều).
    pub fn find_by_request_id(&self, request_id: i64) -> Result<Option<TcocRequest>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!(
            "SELECT {} FROM {} WHERE REQUEST_ID = {} ORDER BY ID DESC",
            TCOC_REQUEST_SELECT,
            self.table_name(),
            request_id
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
}

impl Repository<TcocRequest, i64> for TcocRequestRepository {
    fn get_pool(&self) -> &Pool<OdbcConnectionManager> {
        &self.pool
    }

    fn table_name(&self) -> &str {
        "MEDIATION_OWNER.TCOC_REQUEST"
    }

    fn primary_key(&self) -> &str {
        "ID"
    }

    fn mapper(&self) -> &dyn RowMapper<TcocRequest> {
        &self.mapper
    }

    fn find_by_id(&self, id: i64) -> Result<Option<TcocRequest>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!(
            "SELECT {} FROM {} WHERE {} = {}",
            TCOC_REQUEST_SELECT,
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

    fn find_all(&self) -> Result<Vec<TcocRequest>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!("SELECT {} FROM {}", TCOC_REQUEST_SELECT, self.table_name());
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
                                tracing::error!(error = %e, "[DB] TCOC_REQUEST map_row failed");
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

    fn build_insert_query(&self, entity: &TcocRequest) -> Result<String, DbError> {
        use crate::db::{format_sql_nullable_i64, format_sql_nullable_string};

        let request_datetime = format_sql_nullable_datetime(&entity.request_datetime);

        Ok(format!(
            "INSERT INTO {} (ID, REQUEST_ID, COMMAND_ID, SESSION_ID, TOLL_ID, ETAG_ID, LANE_ID, PLATE, CONTENT, DESCRIPTION, REQUEST_DATETIME, TOLL_IN, TOLL_OUT, IMAGE_COUNT, TID, NODE_ID) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
            self.table_name(),
            format_sql_nullable_i64(&entity.id),
            format_sql_nullable_i64(&entity.request_id),
            format_sql_nullable_i64(&entity.command_id),
            format_sql_nullable_i64(&entity.session_id),
            format_sql_nullable_i64(&entity.toll_id),
            format_sql_nullable_string(&entity.etag_id),
            format_sql_nullable_i64(&entity.lane_id),
            format_sql_nullable_string(&entity.plate),
            format_sql_nullable_string(&entity.content),
            format_sql_nullable_string(&entity.description),
            request_datetime,
            format_sql_nullable_i64(&entity.toll_in),
            format_sql_nullable_i64(&entity.toll_out),
            format_sql_nullable_i64(&entity.image_count),
            format_sql_nullable_string(&entity.tid),
            format_sql_nullable_string(&entity.node_id),
        ))
    }

    fn build_update_query(&self, id: i64, entity: &TcocRequest) -> Result<String, DbError> {
        use crate::db::{format_sql_nullable_i64, format_sql_nullable_string, format_sql_value};

        let request_datetime = format_sql_nullable_datetime(&entity.request_datetime);

        Ok(format!(
            "UPDATE {} SET REQUEST_ID = {}, COMMAND_ID = {}, SESSION_ID = {}, TOLL_ID = {}, ETAG_ID = {}, LANE_ID = {}, PLATE = {}, CONTENT = {}, DESCRIPTION = {}, REQUEST_DATETIME = {}, TOLL_IN = {}, TOLL_OUT = {}, IMAGE_COUNT = {}, TID = {}, NODE_ID = {} WHERE {} = {}",
            self.table_name(),
            format_sql_nullable_i64(&entity.request_id),
            format_sql_nullable_i64(&entity.command_id),
            format_sql_nullable_i64(&entity.session_id),
            format_sql_nullable_i64(&entity.toll_id),
            format_sql_nullable_string(&entity.etag_id),
            format_sql_nullable_i64(&entity.lane_id),
            format_sql_nullable_string(&entity.plate),
            format_sql_nullable_string(&entity.content),
            format_sql_nullable_string(&entity.description),
            request_datetime,
            format_sql_nullable_i64(&entity.toll_in),
            format_sql_nullable_i64(&entity.toll_out),
            format_sql_nullable_i64(&entity.image_count),
            format_sql_nullable_string(&entity.tid),
            format_sql_nullable_string(&entity.node_id),
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
            "MEDIATION_OWNER",
            "TCOC_REQUEST_LONG_SEQ",
        )
    }

    /// Override insert to auto-fetch ID from sequence if not set
    fn insert(&self, entity: &TcocRequest) -> Result<i64, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        // If ID not set, get from sequence
        let mut entity_to_insert = entity.clone();
        if entity_to_insert.id.is_none() {
            use crate::db::sequence::get_next_sequence_value_with_schema;
            entity_to_insert.id = Some(get_next_sequence_value_with_schema(
                pool,
                "MEDIATION_OWNER",
                "TCOC_REQUEST_LONG_SEQ",
            )?);
        }

        let query = self.build_insert_query(&entity_to_insert)?;

        // Execute INSERT query
        let cursor_result = conn.execute(&query, (), None)?;

        // If cursor exists, close it
        if let Some(_cursor) = cursor_result {
            // Cursor is dropped automatically
        }

        // Return the set ID
        entity_to_insert
            .id
            .ok_or_else(|| DbError::Other("Failed to get ID from sequence".to_string()))
    }
}

impl TcocRequestRepository {
    /// Save TCOC_REQUEST info when receiving message from FE
    #[allow(dead_code)]
    pub fn save_request(
        &self,
        fe_request: &FE_REQUEST,
        conn_id: i32,
        data: &[u8],
        toll_id: Option<i64>,
        etag_id: Option<String>,
        lane_id: Option<i64>,
        plate: Option<String>,
        tid: Option<String>,
        node_id: Option<String>,
    ) -> Result<i64, Box<dyn Error>> {
        // Format datetime cho Altibase
        let request_datetime = crate::utils::now_utc_db_string();

        // Content = full hex của bản tin (lưu đầy đủ để theo dõi/audit).
        let content = hex::encode(data);

        let tcoc_request = TcocRequest {
            id: None, // ID sẽ được tự động lấy từ sequence TCOC_REQUEST_LONG_SEQ
            request_id: Some(fe_request.request_id),
            command_id: Some(fe_request.command_id as i64),
            session_id: Some(fe_request.session_id),
            toll_id,
            etag_id,
            lane_id,
            plate,
            content: Some(content),
            description: Some(format!("FE_REQUEST from connection {}", conn_id)),
            request_datetime: Some(request_datetime),
            toll_in: None,
            toll_out: None,
            image_count: None,
            tid,
            node_id,
        };

        // Save to database
        match self.insert(&tcoc_request) {
            Ok(id) => {
                tracing::info!(
                    request_id = fe_request.request_id,
                    id,
                    "[DB] TCOC_REQUEST saved"
                );
                Ok(id)
            }
            Err(e) => {
                tracing::error!(request_id = fe_request.request_id, error = %e, "[DB] TCOC_REQUEST save failed");
                Err(Box::new(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requires MEDIATION_DB_* env and live DB. Run: cargo test -- --ignored
    #[test]
    #[ignore = "requires MEDIATION_DB_* env and live DB"]
    fn test_tcoc_request_repository() {
        let repo = TcocRequestRepository::new();
        assert_eq!(repo.table_name(), "MEDIATION_OWNER.TCOC_REQUEST");
        assert_eq!(repo.primary_key(), "ID");
    }
}
