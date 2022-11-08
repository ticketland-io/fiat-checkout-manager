use eyre::Result;
use borsh::{BorshSerialize};
use amqp_helpers::producer::retry_producer::RetryProducer;
use crate::models::checkout_session::CheckoutSession;

pub struct CheckoutSessionProducer {
  producer: RetryProducer,
}

impl CheckoutSessionProducer {
  pub async fn new(rabbitmq_uri: String, retry_ttl: u16,) -> Self {
    let producer = RetryProducer::new(
      &rabbitmq_uri,
      &"checkout_session_created",
      &"checkout_session_created",
      &"checkout_session_created.new",
      retry_ttl,
    ).await.unwrap();

    Self {
      producer,
    }
  }

  pub async fn new_checkout_session(&self, msg: CheckoutSession) -> Result<()> {
    self.producer.publish(
      &"checkout_session_created",
      &"checkout_session_created.new",
      &msg.try_to_vec()?
    ).await
  }
}
