use std::sync::Arc;
use eyre::Result;
use tracing::info;
use async_trait::async_trait;
use lapin::{
  message::{Delivery},
};
use amqp_helpers::core::types::Handler;
use crate::{
  models::create_checkout::CreateCheckout,
  utils::store::Store,
  services::stripe::{create_primary_sale_checkout},
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

  async fn reserve_seat(&self) -> Result<()> {
    todo!()
  }

  async fn create_checkout_session(&self, msg: &CreateCheckout) -> Result<String> {
    Ok(
      create_primary_sale_checkout(
        Arc::clone(&self.store),
        msg.buyer_uid.clone(),
        msg.sale_account.clone(),
        msg.event_id.clone(),
        msg.ticket_nft.clone(),
        msg.ticket_type_index,
        msg.recipient.clone(),
        msg.seat_index,
        msg.seat_name.clone(),
      ).await?
    )
  }
}

#[async_trait]
impl Handler<CreateCheckout> for CreateCheckoutHandler {
  async fn handle(&self, msg: CreateCheckout, _: &Delivery) -> Result<()> {
    info!("Creating new checkout for user {} and ticket {} from event {}", msg.buyer_uid, msg.ticket_nft, msg.event_id);
    
    self.reserve_seat().await?;
    let _checkout_session_id = self.create_checkout_session(&msg).await?;

    Ok(())
  }
}
