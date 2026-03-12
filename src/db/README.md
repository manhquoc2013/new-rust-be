# Database Repository Pattern

> **Lưu ý**: Tài liệu này tập trung vào Database Repository Pattern và cách sử dụng. Để hiểu về kiến trúc hệ thống tổng thể và cách database được tích hợp, xem [FLOW_DOCUMENTATION.md](../../FLOW_DOCUMENTATION.md#8-database-srcdb).

Pattern này cung cấp một cách tiếp cận khoa học và tối ưu để quản lý giao tiếp với các bảng trong database.

## Cấu trúc

```dic
src/db/
├── mod.rs                    # Module exports
├── error.rs                  # Error types cho database operations
├── mapper.rs                 # Trait và helper functions để map rows thành entities
├── repository.rs             # Repository trait với các method CRUD
├── sequence.rs               # Helper lấy sequence (get_next_sequence_value, ...)
├── examples.rs               # Ví dụ implementation (tham khảo)
├── repositories/             # Các repository implementations
│   ├── mod.rs
│   ├── tcoc_sessions.rs      # TCOC_SESSIONS
│   ├── tcoc_request.rs       # TCOC_REQUEST
│   ├── tcoc_response.rs      # TCOC_RESPONSE
│   ├── tcoc_users.rs         # TCOC_USERS
│   ├── tcoc_connection_server.rs
│   ├── boo_transport_trans_stage.rs
│   ├── boo_trans_stage_tcd.rs
│   ├── transport_transaction_stage.rs
│   ├── transport_trans_stage_tcd.rs
│   ├── subscriber_repository.rs
│   └── ...
└── README.md
```

**Connection pool:** Sử dụng `MEDIATION_DB`, `RATING_DB`, `CRM_DB` từ `crate::configs` (mediation_db.rs, rating_db.rs, crm_db.rs). Manager: `crate::configs::pool_factory::OdbcConnectionManager`.

## Cách sử dụng

### 1. Tạo Entity struct

```rust
#[derive(Debug, Clone)]
pub struct MyEntity {
    pub id: i32,
    pub name: String,
    pub status: Option<i32>,
}
```

### 2. Tạo Mapper

```rust
use crate::db::{RowMapper, DbError, get_i32, get_string, get_nullable_i32};
use odbc_api::CursorRow;

pub struct MyEntityMapper;

impl RowMapper<MyEntity> for MyEntityMapper {
    fn map_row(&self, row: &mut CursorRow) -> Result<MyEntity, DbError> {
        Ok(MyEntity {
            id: get_i32(row, 1)?,
            name: get_string(row, 2)?,
            status: get_nullable_i32(row, 3)?,
        })
    }
}
```

### 3. Tạo Repository

```rust
use crate::db::{Repository, DbError};
use crate::configs::mediation_db::MEDIATION_DB;  // hoặc rating_db::RATING_DB, crm_db::CRM_DB
use crate::configs::pool_factory::OdbcConnectionManager;
use r2d2::Pool;

pub struct MyEntityRepository {
    pool: Pool<OdbcConnectionManager>,
    mapper: MyEntityMapper,
}

impl MyEntityRepository {
    pub fn new() -> Self {
        Self {
            pool: MEDIATION_DB.clone(),
            mapper: MyEntityMapper,
        }
    }
}

impl Repository<MyEntity, i32> for MyEntityRepository {
    fn get_pool(&self) -> &Pool<OdbcConnectionManager> {
        &self.pool
    }
    
    fn table_name(&self) -> &str {
        "MEDIATION_OWNER.MY_TABLE"
    }
    
    fn primary_key(&self) -> &str {
        "ID"
    }
    
    fn mapper(&self) -> &dyn RowMapper<MyEntity> {
        &self.mapper
    }
    
    fn build_insert_query(&self, entity: &MyEntity) -> Result<String, DbError> {
        use crate::db::{format_sql_string, format_sql_value, format_sql_nullable_string};
        
        let status_str = entity.status.map(|s| s.to_string());
        
        Ok(format!(
            "INSERT INTO {} (NAME, STATUS) VALUES ({}, {})",
            self.table_name(),
            format_sql_string(&entity.name),
            format_sql_nullable_string(&status_str),
        ))
    }
    
    fn build_update_query(&self, id: i32, entity: &MyEntity) -> Result<String, DbError> {
        use crate::db::{format_sql_string, format_sql_value, format_sql_nullable_string};
        
        let status_str = entity.status.map(|s| s.to_string());
        
        Ok(format!(
            "UPDATE {} SET NAME = {}, STATUS = {} WHERE {} = {}",
            self.table_name(),
            format_sql_string(&entity.name),
            format_sql_nullable_string(&status_str),
            self.primary_key(),
            format_sql_value(&id),
        ))
    }
    
    fn get_last_insert_id(&self, conn: &mut odbc_api::Connection<'static>) -> Result<i32, DbError> {
        // Implement based on your database
        // For Altibase, you might use a sequence or SELECT MAX(id)
        let query = format!("SELECT MAX({}) FROM {}", self.primary_key(), self.table_name());
        // ... implementation
        Ok(0) // placeholder
    }
}
```

### 4. Sử dụng Repository

```rust
let repo = MyEntityRepository::new();

// Lấy tất cả records
let all = repo.find_all()?;

// Lấy theo ID
let entity = repo.find_by_id(1)?;

// Thêm mới
let new_entity = MyEntity {
    id: 0,
    name: "Test".to_string(),
    status: Some(1),
};
let id = repo.insert(&new_entity)?;

// Cập nhật
let updated = MyEntity {
    id: 1,
    name: "Updated".to_string(),
    status: Some(2),
};
repo.update(1, &updated)?;

// Xóa
repo.delete(1)?;

// Đếm
let count = repo.count()?;
```

## Các method có sẵn

- `find_all()` - Lấy tất cả records
- `find_by_id(id)` - Lấy 1 record theo ID
- `insert(entity)` - Thêm mới record
- `update(id, entity)` - Cập nhật record
- `delete(id)` - Xóa record
- `count()` - Đếm tổng số records

## Helper functions

- `get_string(row, index)` - Lấy string từ row
- `get_i32(row, index)` - Lấy i32 từ row
- `get_i64(row, index)` - Lấy i64 từ row
- `get_nullable_string(row, index)` - Lấy Option<String> từ row
- `get_nullable_i32(row, index)` - Lấy Option<i32> từ row
- `get_nullable_i64(row, index)` - Lấy Option<i64> từ row
- `format_sql_string(value)` - Format string cho SQL (với escaping)
- `format_sql_value(value)` - Format value cho SQL
- `format_sql_nullable_string(value)` - Format Option<String> cho SQL
- `format_sql_nullable_i64(value)` - Format Option<i64> cho SQL
- `format_sql_nullable_i32(value)` - Format Option<i32> cho SQL

## Lưu ý

1. Column index bắt đầu từ 1 (không phải 0)
2. Cần implement `build_insert_query`, `build_update_query`, và `get_last_insert_id` cho mỗi repository
3. SQL injection được ngăn chặn bằng cách escape strings trong helper functions
4. Connection pool được quản lý tự động

## Required sequences (MEDIATION_OWNER)

ROAMING_REQUEST và ROAMING_RESPONSE dùng sequence để sinh ID. Nếu log báo lỗi **"Column not found"** khi gọi `ROAMING_REQUEST_SEQ.NEXTVAL` hoặc `ROAMING_RESPONSE_SEQ.NEXTVAL`, nghĩa là các sequence này chưa được tạo trong schema MEDIATION_OWNER.

**Cần tạo hai sequence:**

- `MEDIATION_OWNER.ROAMING_REQUEST_SEQ`
- `MEDIATION_OWNER.ROAMING_RESPONSE_SEQ`

DDL mẫu (Altibase): xem và chạy file [sql/mediation_owner_sequences.sql](../../sql/mediation_owner_sequences.sql).

## Required sequences (RATING_OWNER)

Các bảng BOO/TRANSPORT stage và history dùng sequence để sinh ID. Nếu log báo **"Column not found"** khi gọi `BOO_TRANS_ST_DETAIL_H_SEQ.NEXTVAL` hoặc **sequence chưa tồn tại**, cần tạo các sequence sau trong schema RATING_OWNER:

- `RATING_OWNER.BOO_TRANS_ST_DETAIL_H_SEQ` — dùng khi ghi BOO_TRANS_STAGE_TCD_HIS (sau mỗi INSERT/UPDATE BOO_TRANS_STAGE_TCD)
- `RATING_OWNER.TRANSPORT_TRANS_STAGE_H_SEQ` — dùng khi ghi BOO_TRANSPORT_TRANS_STAGE_H (sau INSERT/UPDATE BOO_TRANSPORT_TRANS_STAGE)
- `RATING_OWNER.BOO_TRANSPORT_TRANS_ST_H_SEQ` — dùng cho BOO_TRANS_STAGE_TCD_ID và BOO_TRANSPORT_TRANS_STAGE
- `RATING_OWNER.TRANSPORT_STAGE_TCD_HIS_SEQ` — dùng cho TRANSPORT_TRANS_STAGE_TCD history (BECT)
- `RATING_OWNER.TRANSPORT_TRANS_STAGE_SEQ` — dùng cho TRANSPORT_TRANSACTION_STAGE (BECT)

**Altibase:** `CREATE SEQUENCE RATING_OWNER.BOO_TRANS_ST_DETAIL_H_SEQ;` (và tương tự cho các sequence khác nếu chưa có).

**Oracle:** `CREATE SEQUENCE RATING_OWNER.BOO_TRANS_ST_DETAIL_H_SEQ;` (cú pháp tương thích).

## Ví dụ thực tế: TCOC_SESSIONS

### Entity đã được tạo sẵn

Hệ thống đã có sẵn Entity, Mapper và Repository cho bảng `MEDIATION_OWNER.TCOC_SESSIONS`:

```rust
use crate::db::{TcocSession, TcocSessionRepository};

// Tạo repository instance
let repo = TcocSessionRepository::new();

// Lấy tất cả sessions
let all_sessions = repo.find_all()?;
println!("Total sessions: {}", all_sessions.len());

// Lấy session theo ID
let session_id = 12345i64;
match repo.find_by_id(session_id)? {
    Some(session) => {
        println!("Found session: {:?}", session);
        println!("User: {:?}, Status: {:?}", session.user_name, session.status);
    }
    None => println!("Session not found"),
}

// Tạo session mới
let new_session = TcocSession {
    session_id: 12345,
    user_id: Some(100),
    init_datetime: Some("2024-01-01 10:00:00".to_string()),
    login_datetime: Some("2024-01-01 10:00:00".to_string()),
    logout_datetime: None,
    valid_code: Some("VALID".to_string()),
    ip_address: Some("192.168.1.1".to_string()),
    user_name: Some("admin".to_string()),
    toll_id: Some(10022),
    status: Some("ACTIVE".to_string()),
    description: Some("Initial login".to_string()),
    server_id: Some(1),
};

// Insert session mới
let inserted_id = repo.insert(&new_session)?;
println!("Inserted session with ID: {}", inserted_id);

// Cập nhật session
let mut updated_session = new_session.clone();
updated_session.status = Some("LOGGED_OUT".to_string());
updated_session.logout_datetime = Some("2024-01-01 18:00:00".to_string());

match repo.update(session_id, &updated_session)? {
    true => println!("Session updated successfully"),
    false => println!("Session update failed or not found"),
}

// Xóa session
match repo.delete(session_id)? {
    true => println!("Session deleted successfully"),
    false => println!("Session delete failed or not found"),
}

// Đếm tổng số sessions
let total_sessions = repo.count()?;
println!("Total sessions in database: {}", total_sessions);
```

### Cấu trúc Entity TCOC_SESSIONS

```rust
pub struct TcocSession {
    pub session_id: i64,                    // Primary Key, NOT NULL
    pub user_id: Option<i64>,               // USER_ID
    pub init_datetime: Option<String>,       // INIT_DATETIME
    pub login_datetime: Option<String>,      // LOGIN_DATETIME
    pub logout_datetime: Option<String>,    // LOGOUT_DATETIME
    pub valid_code: Option<String>,         // VALID_CODE (VARCHAR(5))
    pub ip_address: Option<String>,        // IP_ADDRESS (VARCHAR(15))
    pub user_name: Option<String>,          // USER_NAME (VARCHAR(100))
    pub toll_id: Option<i64>,               // TOLL_ID
    pub status: Option<String>,             // STATUS (VARCHAR(196))
    pub description: Option<String>,        // DESCRIPTION (VARCHAR(1000))
    pub server_id: Option<i64>,            // SERVER_ID
}
```

### Schema Database

```sql
CREATE TABLE "MEDIATION_OWNER"."TCOC_SESSIONS"
(
 "SESSION_ID" FLOAT FIXED NOT NULL,
 "USER_ID" FLOAT FIXED,
 "INIT_DATETIME" DATE FIXED,
 "LOGIN_DATETIME" DATE FIXED,
 "LOGOUT_DATETIME" DATE FIXED,
 "VALID_CODE" VARCHAR(5) FIXED,
 "IP_ADDRESS" VARCHAR(15) FIXED,
 "USER_NAME" VARCHAR(100) VARIABLE,
 "TOLL_ID" FLOAT FIXED,
 "STATUS" VARCHAR(196) VARIABLE,
 "DESCRIPTION" VARCHAR(1000) VARIABLE,
 "SERVER_ID" FLOAT FIXED,
 CONSTRAINT "TCOC_SESSIONS_PK" PRIMARY KEY ("SESSION_ID")
)
```

### Các method có sẵn cho TCOC_SESSIONS

- `TcocSessionRepository::new()` - Tạo repository instance
- `repo.find_all()` - Lấy tất cả sessions
- `repo.find_by_id(session_id)` - Lấy session theo SESSION_ID
- `repo.insert(session)` - Thêm session mới
- `repo.update(session_id, session)` - Cập nhật session
- `repo.delete(session_id)` - Xóa session
- `repo.count()` - Đếm tổng số sessions

### Import

```rust
use crate::db::{TcocSession, TcocSessionRepository, TcocSessionMapper};
```

Các repository khác có sẵn (Entity, Mapper, Repository): `TcocRequest`, `TcocResponse`, `TcocUser`, `TcocConnectionServer`, `BooTransportTransStage`, `BooTransStageTcd`, `TransportTransactionStage`, `TransportTransStageTcd`. Xem `src/db/mod.rs` và `src/db/repositories/mod.rs` để biết đầy đủ.
