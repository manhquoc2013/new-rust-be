//! Business logic: request processor receives RequestToProcess from reader channel, runs process_request in parallel, sends ReplyToRoute to router.

pub mod handler;

pub use handler::run_request_processor;
