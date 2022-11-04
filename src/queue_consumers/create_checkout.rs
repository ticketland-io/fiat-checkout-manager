use std::sync::Arc;
use eyre::{Result, Report};
use tracing::info;
use async_trait::async_trait;
use lapin::{
  message::{Delivery},
};
use amqp_helpers::core::types::Handler;
use crate::{
  models::create_checkout::CreateCheckout,
  utils::store::Store,
};

pub struct CreateCheckoutHandler {
  store: Arc<Store>
}

impl CreateCheckoutHandler {
  pub async fn new(store: Arc<Store>) -> Self {
    Self {
      store,
    }
  }
}

#[async_trait]
impl Handler<CreateCheckout> for CreateCheckoutHandler {
  async fn handle(&self, msg: CreateCheckout, _: &Delivery) -> Result<()> {
    info!("Creating new checkout for user {} and ticket {} from event {}", msg.buyer_uid, msg.ticket_nft, msg.event_id);

    Ok(())
  }
}
