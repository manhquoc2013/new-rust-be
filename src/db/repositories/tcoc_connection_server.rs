//! Entity, mapper, repository for table TCOC_CONNECTION_SERVER (tra cứu theo IP).

use crate::configs::mediation_db::MEDIATION_DB;
use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::{
    get_i64, get_nullable_i64, get_nullable_string, get_string, DbError, Repository, RowMapper,
};
use odbc_api::{Cursor, CursorRow};
use r2d2::Pool;

/// Entity for table TCOC_CONNECTION_SERVER
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TcocConnectionServer {
    pub server_id: i64,                 // SERVER_ID - Primary Key, NOT NULL
    pub name: String,                   // NAME - NOT NULL
    pub ip: String,                     // IP - NOT NULL
    pub status: String,                 // STATUS - NOT NULL
    pub toll_profile_id: Option<i64>,   // TOLL_PROFILE_ID - nullable
    pub encryption_key: Option<String>, // ENCRYPTION_KEY - nullable
    pub toll_id: Option<String>,        // TOLL_ID - nullable
    pub channel_type: Option<String>,   // CHANNEL_TYPE - nullable
}

/// Mapper cho TCOC_CONNECTION_SERVER
pub struct TcocConnectionServerMapper;

impl RowMapper<TcocConnectionServer> for TcocConnectionServerMapper {
    fn map_row(&self, row: &mut CursorRow) -> Result<TcocConnectionServer, DbError> {
        Ok(TcocConnectionServer {
            server_id: get_i64(row, 1)?,                  // SERVER_ID
            name: get_string(row, 2)?,                    // NAME
            ip: get_string(row, 3)?,                      // IP
            status: get_string(row, 4)?,                  // STATUS
            toll_profile_id: get_nullable_i64(row, 5)?,   // TOLL_PROFILE_ID
            encryption_key: get_nullable_string(row, 6)?, // ENCRYPTION_KEY
            toll_id: get_nullable_string(row, 7)?,        // TOLL_ID
            channel_type: get_nullable_string(row, 8)?,   // CHANNEL_TYPE
        })
    }
}

/// Repository cho TCOC_CONNECTION_SERVER
pub struct TcocConnectionServerRepository {
    pool: Pool<OdbcConnectionManager>,
    mapper: TcocConnectionServerMapper,
}

impl TcocConnectionServerRepository {
    pub fn new() -> Self {
        Self {
            pool: MEDIATION_DB.clone(),
            mapper: TcocConnectionServerMapper,
        }
    }

    /// Create repository using pool from holder (dùng trong db_retry khi MEDIATION init lỗi rồi reconnect qua holder).
    pub fn with_pool(pool: &Pool<OdbcConnectionManager>) -> Self {
        Self {
            pool: pool.clone(),
            mapper: TcocConnectionServerMapper,
        }
    }

    /// Find all rows using the given pool (used when pool from holder/retry, không dùng MEDIATION_DB).
    pub fn find_all_with_pool(
        pool: &Pool<OdbcConnectionManager>,
    ) -> Result<Vec<TcocConnectionServer>, DbError> {
        let conn = get_connection_with_retry(pool)?;
        let query = format!(
            "SELECT * FROM {} WHERE STATUS = '1'",
            "MEDIATION_OWNER.TCOC_CONNECTION_SERVER"
        );
        let mapper = TcocConnectionServerMapper;
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
                            )));
                        }
                    }
                }
                Ok(results)
            }
            None => Ok(Vec::new()),
        }
    }

    /// Find by IP address
    /// Find by IP address
    pub fn find_by_ip(&self, ip: &str) -> Result<Option<TcocConnectionServer>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        // Escape single quotes in IP to prevent SQL injection
        let escaped_ip = ip.replace("'", "''");
        let query = format!(
            "SELECT * FROM {} WHERE IP = '{}' AND STATUS = '1'",
            self.table_name(),
            escaped_ip
        );
        let mapper = self.mapper();

        let cursor_result = match conn.execute(&query, (), None) {
            Ok(res) => res,
            Err(e) => {
                return Err(DbError::ExecutionError(format!(
                    "Query execution failed: {}",
                    e
                )))
            }
        };

        match cursor_result {
            Some(mut cursor) => match cursor.next_row() {
                Ok(Some(mut row)) => match mapper.map_row(&mut row) {
                    Ok(entity) => Ok(Some(entity)),
                    Err(e) => Err(e),
                },
                Ok(None) => Ok(None),
                Err(e) => Err(DbError::ExecutionError(format!(
                    "Error fetching row: {}",
                    e
                ))),
            },
            None => Ok(None),
        }
    }

    /// Find all records by IP (nhiều record cùng IP).
    pub fn find_all_by_ip(&self, ip: &str) -> Result<Vec<TcocConnectionServer>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;
        let escaped_ip = ip.replace("'", "''");
        let query = format!(
            "SELECT * FROM {} WHERE IP = '{}' AND STATUS = '1'",
            self.table_name(),
            escaped_ip
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
                            Err(e) => return Err(e),
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

impl Repository<TcocConnectionServer, i64> for TcocConnectionServerRepository {
    fn get_pool(&self) -> &Pool<OdbcConnectionManager> {
        &self.pool
    }

    fn table_name(&self) -> &str {
        "MEDIATION_OWNER.TCOC_CONNECTION_SERVER"
    }

    fn primary_key(&self) -> &str {
        "SERVER_ID"
    }

    fn mapper(&self) -> &dyn RowMapper<TcocConnectionServer> {
        &self.mapper
    }

    fn build_insert_query(&self, entity: &TcocConnectionServer) -> Result<String, DbError> {
        use crate::db::{
            format_sql_nullable_i64, format_sql_nullable_string, format_sql_string,
            format_sql_value,
        };

        Ok(format!(
            "INSERT INTO {} (SERVER_ID, NAME, IP, STATUS, TOLL_PROFILE_ID, ENCRYPTION_KEY, TOLL_ID, CHANNEL_TYPE) VALUES ({}, {}, {}, {}, {}, {}, {}, {})",
            self.table_name(),
            format_sql_value(&entity.server_id),
            format_sql_string(&entity.name),
            format_sql_string(&entity.ip),
            format_sql_string(&entity.status),
            format_sql_nullable_i64(&entity.toll_profile_id),
            format_sql_nullable_string(&entity.encryption_key),
            format_sql_nullable_string(&entity.toll_id),
            format_sql_nullable_string(&entity.channel_type),
        ))
    }

    fn build_update_query(
        &self,
        id: i64,
        entity: &TcocConnectionServer,
    ) -> Result<String, DbError> {
        use crate::db::{format_sql_nullable_i64, format_sql_nullable_string, format_sql_string};

        let mut set_parts = Vec::new();

        // NAME is NOT NULL, always update
        set_parts.push(format!("NAME = {}", format_sql_string(&entity.name)));

        // IP is NOT NULL, always update
        set_parts.push(format!("IP = {}", format_sql_string(&entity.ip)));

        // STATUS is NOT NULL, always update
        set_parts.push(format!("STATUS = {}", format_sql_string(&entity.status)));

        // Optional fields
        if let Some(ref _toll_profile_id) = entity.toll_profile_id {
            set_parts.push(format!(
                "TOLL_PROFILE_ID = {}",
                format_sql_nullable_i64(&entity.toll_profile_id)
            ));
        } else {
            set_parts.push("TOLL_PROFILE_ID = NULL".to_string());
        }

        if let Some(ref _encryption_key) = entity.encryption_key {
            set_parts.push(format!(
                "ENCRYPTION_KEY = {}",
                format_sql_nullable_string(&entity.encryption_key)
            ));
        } else {
            set_parts.push("ENCRYPTION_KEY = NULL".to_string());
        }

        if let Some(ref _toll_id) = entity.toll_id {
            set_parts.push(format!(
                "TOLL_ID = {}",
                format_sql_nullable_string(&entity.toll_id)
            ));
        } else {
            set_parts.push("TOLL_ID = NULL".to_string());
        }

        if let Some(ref _channel_type) = entity.channel_type {
            set_parts.push(format!(
                "CHANNEL_TYPE = {}",
                format_sql_nullable_string(&entity.channel_type)
            ));
        } else {
            set_parts.push("CHANNEL_TYPE = NULL".to_string());
        }

        Ok(format!(
            "UPDATE {} SET {} WHERE SERVER_ID = {}",
            self.table_name(),
            set_parts.join(", "),
            id
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requires MEDIATION_DB_* env and live DB. Run: cargo test -- --ignored
    #[test]
    #[ignore = "requires MEDIATION_DB_* env and live DB"]
    fn test_tcoc_connection_server_repository() {
        let repo = TcocConnectionServerRepository::new();
        assert_eq!(repo.table_name(), "MEDIATION_OWNER.TCOC_CONNECTION_SERVER");
        assert_eq!(repo.primary_key(), "SERVER_ID");
    }
}
