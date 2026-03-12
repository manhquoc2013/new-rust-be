//! Entity, mapper, repository for table TCOC_SESSIONS.

use crate::configs::mediation_db::MEDIATION_DB;
use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::repository::format_sql_nullable_datetime;
use crate::db::{
    get_i64, get_nullable_datetime_string, get_nullable_i64, get_nullable_string, DbError,
    Repository, RowMapper,
};
use odbc_api::{Cursor, CursorRow};
use r2d2::Pool;

/// Entity for table TCOC_SESSIONS
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TcocSession {
    pub session_id: i64,
    pub user_id: Option<i64>,
    pub init_datetime: Option<String>,
    pub login_datetime: Option<String>,
    pub logout_datetime: Option<String>,
    pub valid_code: Option<String>,
    pub ip_address: Option<String>,
    pub user_name: Option<String>,
    pub toll_id: Option<i64>,
    pub status: Option<String>,
    pub description: Option<String>,
    pub server_id: Option<i64>,
}

/// Mapper cho TCOC_SESSIONS
pub struct TcocSessionMapper;

impl RowMapper<TcocSession> for TcocSessionMapper {
    fn map_row(&self, row: &mut CursorRow) -> Result<TcocSession, DbError> {
        Ok(TcocSession {
            session_id: get_i64(row, 1)?,                           // SESSION_ID
            user_id: get_nullable_i64(row, 2)?,                     // USER_ID
            init_datetime: get_nullable_datetime_string(row, 3)?,   // INIT_DATETIME
            login_datetime: get_nullable_datetime_string(row, 4)?,  // LOGIN_DATETIME
            logout_datetime: get_nullable_datetime_string(row, 5)?, // LOGOUT_DATETIME
            valid_code: get_nullable_string(row, 6)?,               // VALID_CODE
            ip_address: get_nullable_string(row, 7)?,               // IP_ADDRESS
            user_name: get_nullable_string(row, 8)?,                // USER_NAME
            toll_id: get_nullable_i64(row, 9)?,                     // TOLL_ID
            status: get_nullable_string(row, 10)?,                  // STATUS
            description: get_nullable_string(row, 11)?,             // DESCRIPTION
            server_id: get_nullable_i64(row, 12)?,                  // SERVER_ID
        })
    }
}

/// SELECT list with TO_CHAR for datetime columns so when reading via ODBC the time part is preserved.
const TCOC_SESSIONS_SELECT: &str = "SESSION_ID, USER_ID, TO_CHAR(INIT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), TO_CHAR(LOGIN_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), TO_CHAR(LOGOUT_DATETIME, 'YYYY-MM-DD HH24:MI:SS'), VALID_CODE, IP_ADDRESS, USER_NAME, TOLL_ID, STATUS, DESCRIPTION, SERVER_ID";

/// Repository cho TCOC_SESSIONS
pub struct TcocSessionRepository {
    pool: Pool<OdbcConnectionManager>,
    mapper: TcocSessionMapper,
}

impl TcocSessionRepository {
    pub fn new() -> Self {
        Self {
            pool: MEDIATION_DB.clone(),
            mapper: TcocSessionMapper,
        }
    }

    /// Create repository using pool from holder (dùng trong db_retry khi MEDIATION init lỗi rồi reconnect qua holder).
    pub fn with_pool(pool: &Pool<OdbcConnectionManager>) -> Self {
        Self {
            pool: pool.clone(),
            mapper: TcocSessionMapper,
        }
    }

    /// Update LOGOUT_DATETIME when disconnecting or closing session.
    pub fn update_logout_datetime(
        &self,
        session_id: i64,
        logout_datetime: &str,
    ) -> Result<(), DbError> {
        use crate::db::repository::format_sql_datetime_str;
        let logout_sql = format_sql_datetime_str(logout_datetime);
        let query = format!(
            "UPDATE {} SET LOGOUT_DATETIME = {} WHERE {} = {}",
            self.table_name(),
            logout_sql,
            self.primary_key(),
            session_id,
        );
        let conn = get_connection_with_retry(self.get_pool())?;
        let _ = conn.execute(&query, (), None)?;
        Ok(())
    }
}

impl Repository<TcocSession, i64> for TcocSessionRepository {
    fn get_pool(&self) -> &Pool<OdbcConnectionManager> {
        &self.pool
    }

    fn table_name(&self) -> &str {
        "MEDIATION_OWNER.TCOC_SESSIONS"
    }

    fn primary_key(&self) -> &str {
        "SESSION_ID"
    }

    fn mapper(&self) -> &dyn RowMapper<TcocSession> {
        &self.mapper
    }

    fn find_by_id(&self, id: i64) -> Result<Option<TcocSession>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!(
            "SELECT {} FROM {} WHERE {} = {}",
            TCOC_SESSIONS_SELECT,
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

    fn find_all(&self) -> Result<Vec<TcocSession>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let query = format!("SELECT {} FROM {}", TCOC_SESSIONS_SELECT, self.table_name());
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
                                tracing::error!(error = %e, "[DB] TCOC_SESSIONS map_row failed");
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

    /// Override insert để tự động lấy SESSION_ID từ sequence nếu chưa được set
    fn insert(&self, entity: &TcocSession) -> Result<i64, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        // Nếu session_id chưa được set (0), lấy từ sequence
        let mut entity_to_insert = entity.clone();
        if entity_to_insert.session_id == 0 {
            use crate::db::sequence::get_next_sequence_value_with_schema;
            entity_to_insert.session_id =
                get_next_sequence_value_with_schema(pool, "MEDIATION_OWNER", "TCOC_SESSIONS_SEQ")?;
        }

        let query = self.build_insert_query(&entity_to_insert)?;

        // Execute INSERT query
        // Trong Altibase, INSERT có thể không trả về cursor nhưng vẫn thành công
        let cursor_result = conn.execute(&query, (), None)?;

        // If cursor exists, close it
        if let Some(_cursor) = cursor_result {
            // Cursor is dropped automatically
        }

        // Trong Altibase, INSERT thành công nếu không có exception
        // Trả về session_id đã được set
        Ok(entity_to_insert.session_id)
    }

    fn build_insert_query(&self, entity: &TcocSession) -> Result<String, DbError> {
        use crate::db::{format_sql_nullable_i64, format_sql_nullable_string, format_sql_value};

        let init_datetime = format_sql_nullable_datetime(&entity.init_datetime);
        let login_datetime = format_sql_nullable_datetime(&entity.login_datetime);
        let logout_datetime = format_sql_nullable_datetime(&entity.logout_datetime);
        // VALID_CODE is VARCHAR(5); truncate to avoid "Invalid data type length" from ODBC/Altibase
        let valid_code_truncated = entity
            .valid_code
            .as_ref()
            .map(|s| s.chars().take(5).collect::<String>());

        Ok(format!(
            "INSERT INTO {} (SESSION_ID, USER_ID, INIT_DATETIME, LOGIN_DATETIME, LOGOUT_DATETIME, VALID_CODE, IP_ADDRESS, USER_NAME, TOLL_ID, STATUS, DESCRIPTION, SERVER_ID) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
            self.table_name(),
            format_sql_value(&entity.session_id),
            format_sql_nullable_i64(&entity.user_id),
            init_datetime,
            login_datetime,
            logout_datetime,
            format_sql_nullable_string(&valid_code_truncated),
            format_sql_nullable_string(&entity.ip_address),
            format_sql_nullable_string(&entity.user_name),
            format_sql_nullable_i64(&entity.toll_id),
            format_sql_nullable_string(&entity.status),
            format_sql_nullable_string(&entity.description),
            format_sql_nullable_i64(&entity.server_id),
        ))
    }

    fn build_update_query(&self, id: i64, entity: &TcocSession) -> Result<String, DbError> {
        use crate::db::{format_sql_nullable_i64, format_sql_nullable_string, format_sql_value};

        let init_datetime = format_sql_nullable_datetime(&entity.init_datetime);
        let login_datetime = format_sql_nullable_datetime(&entity.login_datetime);
        let logout_datetime = format_sql_nullable_datetime(&entity.logout_datetime);
        // VALID_CODE is VARCHAR(5); truncate to avoid "Invalid data type length" from ODBC/Altibase
        let valid_code_truncated = entity
            .valid_code
            .as_ref()
            .map(|s| s.chars().take(5).collect::<String>());

        Ok(format!(
            "UPDATE {} SET USER_ID = {}, INIT_DATETIME = {}, LOGIN_DATETIME = {}, LOGOUT_DATETIME = {}, VALID_CODE = {}, IP_ADDRESS = {}, USER_NAME = {}, TOLL_ID = {}, STATUS = {}, DESCRIPTION = {}, SERVER_ID = {} WHERE {} = {}",
            self.table_name(),
            format_sql_nullable_i64(&entity.user_id),
            init_datetime,
            login_datetime,
            logout_datetime,
            format_sql_nullable_string(&valid_code_truncated),
            format_sql_nullable_string(&entity.ip_address),
            format_sql_nullable_string(&entity.user_name),
            format_sql_nullable_i64(&entity.toll_id),
            format_sql_nullable_string(&entity.status),
            format_sql_nullable_string(&entity.description),
            format_sql_nullable_i64(&entity.server_id),
            self.primary_key(),
            format_sql_value(&id),
        ))
    }

    fn get_last_insert_id(
        &self,
        _conn: &mut odbc_api::Connection<'static>,
    ) -> Result<i64, DbError> {
        // Use sequence to get next ID
        // Nếu entity đã có session_id được set, trả về giá trị đó
        // Nếu không, lấy từ sequence
        use crate::db::sequence::get_next_sequence_value_with_schema;
        get_next_sequence_value_with_schema(self.get_pool(), "MEDIATION_OWNER", "TCOC_SESSIONS_SEQ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requires MEDIATION_DB_* env and live DB. Run: cargo test -- --ignored
    #[test]
    #[ignore = "requires MEDIATION_DB_* env and live DB"]
    fn test_tcoc_session_repository() {
        let repo = TcocSessionRepository::new();
        assert_eq!(repo.table_name(), "MEDIATION_OWNER.TCOC_SESSIONS");
        assert_eq!(repo.primary_key(), "SESSION_ID");
    }
}
