//! Kafka producer: queue, service, types.
//! Khi Kafka chưa ready: event đẩy vào pending queue, khi producer ready sẽ drain vào queue của producer.

mod service;
mod types;

pub(crate) use service::KAFKA_PRODUCER;
pub use service::{
    drain_pending_into_producer, get_kafka_producer, init_pending_queue, push_pending_checkin,
    push_pending_checkout, KafkaProducerService, PendingKafkaQueue,
};
pub use types::{CheckinCommitPayload, CheckoutCommitPayload};
