//! DB access: repository trait, mapper, sequence, concrete repositories (TCOC, roaming, transport, ...).
#![allow(warnings)]

pub mod error;
pub mod mapper;
pub mod repositories;
pub mod repository;
pub mod sequence;

pub use error::DbError;
pub use mapper::{
    get_i32, get_i64, get_nullable_datetime_string, get_nullable_f64, get_nullable_i32,
    get_nullable_i64, get_nullable_string, get_string, RowMapper,
};

pub use repositories::{
    TcocConnectionServer, TcocConnectionServerMapper, TcocConnectionServerRepository, TcocRequest,
    TcocRequestMapper, TcocRequestRepository, TcocResponse, TcocResponseMapper,
    TcocResponseRepository, TcocSession, TcocSessionMapper, TcocSessionRepository, TcocUser,
    TcocUserMapper, TcocUserRepository, TransportTransStageSync, TransportTransStageSyncDt,
    TransportTransStageSyncDtMapper, TransportTransStageSyncDtRepository,
    TransportTransStageSyncMapper, TransportTransStageSyncRepository, TransportTransStageTcd,
    TransportTransStageTcdMapper, TransportTransStageTcdRepository, TransportTransactionStage,
    TransportTransactionStageMapper, TransportTransactionStageRepository,
};
pub use repository::{
    escape_sql_string, format_sql_nullable_f64, format_sql_nullable_i32, format_sql_nullable_i64,
    format_sql_nullable_string, format_sql_string, format_sql_value, Repository,
};
pub use sequence::{
    get_current_sequence_value, get_current_sequence_value_with_schema, get_next_sequence_value,
    get_next_sequence_value_with_schema,
};
