//! Service layer: cache trait, TCOC/transport/toll_fee services.
#![allow(warnings)]

pub mod cache;
pub mod ip_block_service;
pub mod service;
pub mod tcoc_connection_server_service;
pub mod tcoc_request_service;
pub mod tcoc_response_service;
pub mod tcoc_session_service;
pub mod tcoc_user_service;
pub mod ticket_id_service;
pub mod toll_fee;
pub mod transport_trans_stage_tcd_service;
pub mod transport_transaction_stage_service;

// Re-export shared types
pub use cache::{DistributedCache, MemoryCache};
pub use ip_block_service::IpBlockService;
pub use tcoc_connection_server_service::TcocConnectionServerService;
pub use tcoc_request_service::TcocRequestService;
pub use tcoc_response_service::TcocResponseService;
pub use tcoc_session_service::TcocSessionService;
pub use tcoc_user_service::TcocUserService;
pub use ticket_id_service::TicketIdService;
pub use transport_trans_stage_tcd_service::TransportTransStageTcdService;
pub use transport_transaction_stage_service::TransportTransactionStageService;
