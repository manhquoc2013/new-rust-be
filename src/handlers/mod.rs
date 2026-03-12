//! Handler FE command handling: connect, handshake, checkin, commit, rollback, terminate, processor.
//! Các bản tin đều là cặp req/resp từ FE: FE gửi req → processor phân command_id → gọi handler → handler trả resp cho FE.

pub mod checkin;
pub mod commit;
pub mod connect;
pub mod handshake;
pub mod processor;
pub mod roaming;
pub mod rollback;
pub mod terminate;
