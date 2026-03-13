//! Handler FE command handling: connect, handshake, checkin, commit, rollback, terminate, query_vehicle_boo, lookup_vehicle, checkout_reserve, checkout_commit, checkout_rollback, processor.
//! Các bản tin đều là cặp req/resp từ FE: FE gửi req → processor phân command_id → gọi handler → handler trả resp cho FE.

pub mod checkin;
pub mod checkout_commit;
pub mod checkout_reserve;
pub mod checkout_rollback;
pub mod commit;
pub mod connect;
pub mod handshake;
pub mod lookup_vehicle;
pub mod processor;
pub mod query_vehicle_boo;
pub mod roaming;
pub mod rollback;
pub mod terminate;
