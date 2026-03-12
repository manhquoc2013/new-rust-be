//! Entity, mapper, repository for table TCOC_RESPONSE.

use crate::configs::mediation_db::MEDIATION_DB;
use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::repository::format_sql_nullable_datetime;
use crate::db::{
    format_sql_nullable_i64, format_sql_nullable_string, get_nullable_datetime_string,
    get_nullable_i64, get_nullable_string, DbError, Repository, RowMapper,
};
use chrono::Utc;
use odbc_api::{Cursor, CursorRow};
use r2d2::Pool;
use std::error::Error;

/// Entity for table TCOC_RESPONSE
#[derive(Debug, Clone)]
pub struct TcocResponse {
    pub id: Option<i64>,
    pub request_id: Option<i64>,
    pub command_id: Option<i64>,
    pub session_id: Option<i64>,
    pub toll_id: Option<i64>,
    pub toll_in: Option<i64>,
    pub toll_out: Option<i64>,
    pub etag_id: Option<String>,
    pub lane_id: Option<i64>,
    pub plate: Option<String>,
    pub content: Option<String>,
    pub description: Option<String>,
    pub response_datetime: Option<String>,
    pub status: Option<String>,
    pub ticket_id: Option<i64>,
    pub process_duration: Option<i64>,
    pub node_id: Option<String>,
}

/// Mapper cho TCOC_RESPONSE
pub struct TcocResponseMapper;

impl RowMapper<TcocResponse> for TcocResponseMapper {
    fn map_row(&self, row: &mut CursorRow) -> Result<TcocResponse, DbError> {
        Ok(TcocResponse {
            id: get_nullable_i64(row, 1)?,                             // ID
            request_id: get_nullable_i64(row, 2)?,                     // REQUEST_ID
            command_id: get_nullable_i64(row, 3)?,                     // COMMAND_ID
            session_id: get_nullable_i64(row, 4)?,                     // SESSION_ID
            toll_id: get_nullable_i64(row, 5)?,                        // TOLL_ID
            toll_in: get_nullable_i64(row, 6)?,                        // TOLL_IN
            toll_out: get_nullable_i64(row, 7)?,                       // TOLL_OUT
            etag_id: get_nullable_string(row, 8)?,                     // ETAG_ID
            lane_id: get_nullable_i64(row, 9)?,                        // LANE_ID
            plate: get_nullable_string(row, 10)?,                      // PLATE
            content: get_nullable_string(row, 11)?,                    // CONTENT
            description: get_nullable_string(row, 12)?,                // DESCRIPTION
            response_datetime: get_nullable_datetime_string(row, 13)?, // RESPONSE_DATETIME
            status: get_nullable_string(row, 14)?,                     // STATUS
            ticket_id: get_nullable_i64(row, 15)?,                     // TICKET_ID
            process_duration: get_nullable_i64(row, 16)?,              // PROCESS_DURATION
            node_id: get_nullable_string(row, 17)?,                    // NODE_ID
        })
    }
}

/// SELECT list with TO_CHAR for datetime columns so when reading via ODBC the time part is preserved.
const TCOC_RESPONSE_SELECT: &str = "ID, REQUEST_ID, COMMAND_ID, SESSION_ID, TOLL_ID, TOLL_IN, TOLL_OUT, ETAG_ID, LANE_ID, PLATE, CONTENT, DESCRIPTION, TO_CHAR(RESPONSE_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), STATUS, TICKET_ID, PROCESS_DURATION, NODE_ID";

/// Repository cho TCOC_RESPONSE
pub struct TcocResponseRepository {
    pool: Pool<OdbcConnectionManager>,
    mapper: TcocResponseMapper,
}

impl TcocResponseRepository {
    pub fn new() -> Self {
        Self {
            pool: MEDIATION_DB.clone(),
            mapper: TcocResponseMapper,
        }
    }
}

impl Repository<TcocResponse, i64> for TcocResponseRepository {
    fn get_pool(&self) -> &Pool<OdbcConnectionManager> {
        &self.pool
    }

    fn table_name(&self) -> &str {
        "MEDIATION_OWNER.TCOC_RESPONSE"
    }

    fn primary_key(&self) -> &str {
        "ID"
    }

    fn mapper(&self) -> &dyn RowMapper<TcocResponse> {
        &self.mapper
    }

    fn find_by_id(&self, id: i64) -> Result<Option<TcocResponse>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!(
            "SELECT {} FROM {} WHERE {} = {}",
            TCOC_RESPONSE_SELECT,
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

    fn find_all(&self) -> Result<Vec<TcocResponse>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!("SELECT {} FROM {}", TCOC_RESPONSE_SELECT, self.table_name());
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
                                tracing::error!(error = %e, "[DB] TCOC_RESPONSE map_row failed");
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

    fn build_insert_query(&self, entity: &TcocResponse) -> Result<String, DbError> {
        let response_datetime = format_sql_nullable_datetime(&entity.response_datetime);

        Ok(format!(
            "INSERT INTO {} (ID, REQUEST_ID, COMMAND_ID, SESSION_ID, TOLL_ID, TOLL_IN, TOLL_OUT, ETAG_ID, LANE_ID, PLATE, CONTENT, DESCRIPTION, RESPONSE_DATETIME, STATUS, TICKET_ID, PROCESS_DURATION, NODE_ID) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
            self.table_name(),
            format_sql_nullable_i64(&entity.id),
            format_sql_nullable_i64(&entity.request_id),
            format_sql_nullable_i64(&entity.command_id),
            format_sql_nullable_i64(&entity.session_id),
            format_sql_nullable_i64(&entity.toll_id),
            format_sql_nullable_i64(&entity.toll_in),
            format_sql_nullable_i64(&entity.toll_out),
            format_sql_nullable_string(&entity.etag_id),
            format_sql_nullable_i64(&entity.lane_id),
            format_sql_nullable_string(&entity.plate),
            format_sql_nullable_string(&entity.content),
            format_sql_nullable_string(&entity.description),
            response_datetime,
            format_sql_nullable_string(&entity.status),
            format_sql_nullable_i64(&entity.ticket_id),
            format_sql_nullable_i64(&entity.process_duration),
            format_sql_nullable_string(&entity.node_id),
        ))
    }

    fn build_update_query(&self, id: i64, entity: &TcocResponse) -> Result<String, DbError> {
        use crate::db::{format_sql_nullable_i64, format_sql_nullable_string, format_sql_value};

        let response_datetime = format_sql_nullable_datetime(&entity.response_datetime);

        Ok(format!(
            "UPDATE {} SET REQUEST_ID = {}, COMMAND_ID = {}, SESSION_ID = {}, TOLL_ID = {}, TOLL_IN = {}, TOLL_OUT = {}, ETAG_ID = {}, LANE_ID = {}, PLATE = {}, CONTENT = {}, DESCRIPTION = {}, RESPONSE_DATETIME = {}, STATUS = {}, TICKET_ID = {}, PROCESS_DURATION = {}, NODE_ID = {} WHERE {} = {}",
            self.table_name(),
            format_sql_nullable_i64(&entity.request_id),
            format_sql_nullable_i64(&entity.command_id),
            format_sql_nullable_i64(&entity.session_id),
            format_sql_nullable_i64(&entity.toll_id),
            format_sql_nullable_i64(&entity.toll_in),
            format_sql_nullable_i64(&entity.toll_out),
            format_sql_nullable_string(&entity.etag_id),
            format_sql_nullable_i64(&entity.lane_id),
            format_sql_nullable_string(&entity.plate),
            format_sql_nullable_string(&entity.content),
            format_sql_nullable_string(&entity.description),
            response_datetime,
            format_sql_nullable_string(&entity.status),
            format_sql_nullable_i64(&entity.ticket_id),
            format_sql_nullable_i64(&entity.process_duration),
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
            "TCOC_RESPONSE_LONG_SEQ",
        )
    }

    /// Override insert to auto-fetch ID from sequence if not set
    fn insert(&self, entity: &TcocResponse) -> Result<i64, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        // If ID not set, get from sequence
        let mut entity_to_insert = entity.clone();
        if entity_to_insert.id.is_none() {
            use crate::db::sequence::get_next_sequence_value_with_schema;
            entity_to_insert.id = Some(get_next_sequence_value_with_schema(
                pool,
                "MEDIATION_OWNER",
                "TCOC_RESPONSE_LONG_SEQ",
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

impl TcocResponseRepository {
    /// Save TCOC_RESPONSE info before returning to FE
    /// process_duration: processing time in milliseconds (từ lúc nhận request đến lúc gửi response)
    pub fn save_response(
        &self,
        request_id: i64,
        command_id: i32,
        session_id: i64,
        response_data: &[u8],
        status: Option<String>,
        ticket_id: Option<i64>,
        toll_id: Option<i64>,
        toll_in: Option<i64>,
        toll_out: Option<i64>,
        etag_id: Option<String>,
        lane_id: Option<i64>,
        plate: Option<String>,
        node_id: Option<String>,
        process_duration: Option<i64>,
    ) -> Result<i64, Box<dyn Error>> {
        // Format datetime cho Altibase
        let response_datetime = crate::utils::now_utc_db_string();

        // Content = full hex của bản tin response (lưu đầy đủ để theo dõi/audit).
        let content = hex::encode(response_data);

        let tcoc_response = TcocResponse {
            id: None, // ID sẽ được tự động lấy từ sequence TCOC_RESPONSE_LONG_SEQ
            request_id: Some(request_id),
            command_id: Some(command_id as i64),
            session_id: Some(session_id),
            toll_id,
            toll_in,
            toll_out,
            etag_id,
            lane_id,
            plate,
            content: Some(content),
            description: Some(format!("Response for request_id {}", request_id)),
            response_datetime: Some(response_datetime),
            status,
            ticket_id,
            process_duration, // Thời gian xử lý tính bằng milliseconds
            node_id,
        };

        // Save to database
        match self.insert(&tcoc_response) {
            Ok(id) => {
                tracing::info!(request_id, id, process_duration_ms = ?process_duration, "[DB] TCOC_RESPONSE saved");
                Ok(id)
            }
            Err(e) => {
                tracing::error!(request_id, error = %e, "[DB] TCOC_RESPONSE save failed");
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
    fn test_tcoc_response_repository() {
        let repo = TcocResponseRepository::new();
        assert_eq!(repo.table_name(), "MEDIATION_OWNER.TCOC_RESPONSE");
        assert_eq!(repo.primary_key(), "ID");
    }
}
