use eyre::Result;
use borsh::{BorshSerialize};
use amqp_helpers::producer::retry_producer::RetryProducer;
use crate::models::payment_intent::PaymentIntent;

pub struct PaymentProducer {
  producer: RetryProducer,
}

impl PaymentProducer {
  pub async fn new(rabbitmq_uri: String, retry_ttl: u32,) -> Self {
    let producer = RetryProducer::new(
      &rabbitmq_uri,
      &"payment_created",
      &"payment_created",
      &"payment_created.new",
      retry_ttl,
      None,
    ).await.unwrap();

    Self {
      producer,
    }
  }

  pub async fn new_payment(&self, msg: PaymentIntent) -> Result<()> {
    self.producer.publish(
      &"payment_created",
      &"payment_created.new",
      &msg.try_to_vec()?,
      true,
    ).await
  }
}
