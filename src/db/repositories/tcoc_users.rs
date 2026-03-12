//! Entity, mapper, repository for table TCOC_USERS.

use crate::configs::mediation_db::MEDIATION_DB;
use crate::configs::pool_factory::{get_connection_with_retry, OdbcConnectionManager};
use crate::db::{
    format_sql_string, format_sql_value, get_i64, get_nullable_i64, get_nullable_string,
    get_string, DbError, Repository, RowMapper,
};
use odbc_api::{Cursor, CursorRow};
use r2d2::Pool;

/// Entity for table TCOC_USERS
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TcocUser {
    pub user_id: i64,                // USER_ID - Primary Key, NOT NULL
    pub user_name: String,           // USER_NAME - NOT NULL
    pub password: String,            // PASSWORD - NOT NULL
    pub description: Option<String>, // DESCRIPTION - nullable
    pub status: Option<String>,      // STATUS - nullable
    pub toll_id: Option<i64>,        // TOLL_ID - nullable
}

/// Mapper cho TCOC_USERS
pub struct TcocUserMapper;

impl RowMapper<TcocUser> for TcocUserMapper {
    fn map_row(&self, row: &mut CursorRow) -> Result<TcocUser, DbError> {
        Ok(TcocUser {
            user_id: get_i64(row, 1)?,                 // USER_ID
            user_name: get_string(row, 2)?,            // USER_NAME
            password: get_string(row, 3)?,             // PASSWORD
            description: get_nullable_string(row, 4)?, // DESCRIPTION
            status: get_nullable_string(row, 5)?,      // STATUS
            toll_id: get_nullable_i64(row, 6)?,        // TOLL_ID
        })
    }
}

/// Repository cho TCOC_USERS
pub struct TcocUserRepository {
    pool: Pool<OdbcConnectionManager>,
    mapper: TcocUserMapper,
}

impl TcocUserRepository {
    pub fn new() -> Self {
        Self {
            pool: MEDIATION_DB.clone(),
            mapper: TcocUserMapper,
        }
    }

    /// Create repository using pool from holder (dùng trong db_retry khi MEDIATION init lỗi rồi reconnect qua holder).
    pub fn with_pool(pool: &Pool<OdbcConnectionManager>) -> Self {
        Self {
            pool: pool.clone(),
            mapper: TcocUserMapper,
        }
    }

    /// Find all rows using the given pool (used when pool from holder/retry, không dùng MEDIATION_DB).
    pub fn find_all_with_pool(
        pool: &Pool<OdbcConnectionManager>,
    ) -> Result<Vec<TcocUser>, DbError> {
        let conn = get_connection_with_retry(pool)?;
        let query = format!("SELECT * FROM {}", "MEDIATION_OWNER.TCOC_USERS");
        let mapper = TcocUserMapper;
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

    /// Find user by user_name and toll_id
    /// Tìm user theo user_name và toll_id
    pub fn find_by_username_and_toll_id(
        &self,
        username: &str,
        toll_id: i64,
    ) -> Result<Option<TcocUser>, DbError> {
        let pool = self.get_pool();
        let conn = get_connection_with_retry(pool)?;

        // Escape single quotes to prevent SQL injection
        let escaped_username = username.replace("'", "''");
        let query = format!(
            "SELECT * FROM {} WHERE USER_NAME = {} AND TOLL_ID = {}",
            self.table_name(),
            format_sql_string(&escaped_username),
            format_sql_value(&toll_id)
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
}

impl Repository<TcocUser, i64> for TcocUserRepository {
    fn get_pool(&self) -> &Pool<OdbcConnectionManager> {
        &self.pool
    }

    fn table_name(&self) -> &str {
        "MEDIATION_OWNER.TCOC_USERS"
    }

    fn primary_key(&self) -> &str {
        "USER_ID"
    }

    fn mapper(&self) -> &dyn RowMapper<TcocUser> {
        &self.mapper
    }

    fn build_insert_query(&self, entity: &TcocUser) -> Result<String, DbError> {
        use crate::db::{
            format_sql_nullable_i64, format_sql_nullable_string, format_sql_string,
            format_sql_value,
        };

        Ok(format!(
            "INSERT INTO {} (USER_ID, USER_NAME, PASSWORD, DESCRIPTION, STATUS, TOLL_ID) VALUES ({}, {}, {}, {}, {}, {})",
            self.table_name(),
            format_sql_value(&entity.user_id),
            format_sql_string(&entity.user_name),
            format_sql_string(&entity.password),
            format_sql_nullable_string(&entity.description),
            format_sql_nullable_string(&entity.status),
            format_sql_nullable_i64(&entity.toll_id),
        ))
    }

    fn build_update_query(&self, id: i64, entity: &TcocUser) -> Result<String, DbError> {
        use crate::db::{format_sql_nullable_i64, format_sql_nullable_string, format_sql_string};

        let mut set_parts = Vec::new();

        // USER_NAME is NOT NULL, always update
        set_parts.push(format!(
            "USER_NAME = {}",
            format_sql_string(&entity.user_name)
        ));

        // PASSWORD is NOT NULL, always update
        set_parts.push(format!(
            "PASSWORD = {}",
            format_sql_string(&entity.password)
        ));

        // Optional fields
        if let Some(ref _description) = entity.description {
            set_parts.push(format!(
                "DESCRIPTION = {}",
                format_sql_nullable_string(&entity.description)
            ));
        } else {
            set_parts.push("DESCRIPTION = NULL".to_string());
        }

        if let Some(ref _status) = entity.status {
            set_parts.push(format!(
                "STATUS = {}",
                format_sql_nullable_string(&entity.status)
            ));
        } else {
            set_parts.push("STATUS = NULL".to_string());
        }

        if let Some(ref _toll_id) = entity.toll_id {
            set_parts.push(format!(
                "TOLL_ID = {}",
                format_sql_nullable_i64(&entity.toll_id)
            ));
        } else {
            set_parts.push("TOLL_ID = NULL".to_string());
        }

        Ok(format!(
            "UPDATE {} SET {} WHERE USER_ID = {}",
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
    fn test_tcoc_user_repository() {
        let repo = TcocUserRepository::new();
        assert_eq!(repo.table_name(), "MEDIATION_OWNER.TCOC_USERS");
        assert_eq!(repo.primary_key(), "USER_ID");
    }
}
