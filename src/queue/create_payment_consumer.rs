use std::{
  sync::Arc,
  str::FromStr,
};
use eyre::{Result, ContextCompat};
use ticketland_api::services::ticket_availability::get_next_seat_index;
use tracing::info;
use chrono::Duration;
use amqp_helpers::core::types::Handler;
use async_trait::async_trait;
use lapin::{
  message::{Delivery},
};
use ticketland_core::async_helpers::with_retry;
use crate::{
  models::{
    create_payment::CreatePayment,
    payment_intent::{PaymentIntent, PaymentSecret},
  },
  utils::store::Store,
  services::stripe::{create_primary_sale_payment, create_secondary_sale_payment},
};

fn is_custom_error(error: &str) -> bool {
  error == "Ticket unavailable"
  || error == "Invalid ticket_nft"
  || error == "Only fixed price ticket types are supported"
  || error == "Listing unavailable"
}

pub struct CreatePaymentHandler {
  store: Arc<Store>
}

impl CreatePaymentHandler {
  pub fn new(store: Arc<Store>) -> Self {
    Self {
      store,
    }
  }

  async fn create_primary_payment(&self, msg: &CreatePayment, seat_index: u32, seat_name: String) -> Result<String> {
    // let (_, buyer_uid, event_id, ticket_type_index, recipient, txb_bytes, signature) = msg.primary();
    let (_, buyer_uid, event_id, ticket_type_index, recipient) = msg.primary();

    Ok(
      create_primary_sale_payment(
        Arc::clone(&self.store),
        buyer_uid.to_string(),
        event_id.to_string(),
        ticket_type_index,
        recipient.to_string(),
        seat_index,
        seat_name,
        // txb_bytes.to_string(),
        // signature.to_string(),
      ).await?
    )
  }

  async fn create_secondary_sale_payment(&self, msg: &CreatePayment) -> Result<String> {
    let (
      _,
      buyer_uid,
      event_id,
      ticket_type_index,
      recipient,
      seat_index,
      cnt_sui_address,
      listing_sui_address,
    ) = msg.secondary();

    Ok(
      create_secondary_sale_payment(
        Arc::clone(&self.store),
        buyer_uid.to_string(),
        event_id.to_string(),
        ticket_type_index,
        recipient.to_string(),
        seat_index,
        cnt_sui_address.to_string(),
        listing_sui_address.to_string(),
      ).await?
    )
  }
}

#[async_trait]
impl Handler<CreatePayment> for CreatePaymentHandler {
  async fn handle(&mut self, msg: CreatePayment, _: &Delivery, _: i64,) -> Result<()> {
    let (ws_session_id, payment_secret) = match msg {
      CreatePayment::Primary {..} => {
        let (ws_session_id, buyer_uid, event_id, ticket_type_index, _) = msg.primary();

        let seat_index = get_next_seat_index(
          &self.store.pg_pool,
          &self.store.redis_pool,
          Arc::clone(&self.store.rpc_client),
          event_id.to_string(),
          ticket_type_index
        ).await?;
        let seat_name = seat_index.to_string();

        info!("Creating new payment for user {} and seat index {} from event {}", buyer_uid, seat_index, event_id);


        match self.create_primary_payment(&msg, seat_index, seat_name.clone()).await {
          Ok(payment_secret) => Ok((ws_session_id, PaymentSecret::Ok(payment_secret))),
          Err(error) => {
            // we don't want to nack if the ticket is unavailable. Instead we need to ack and
            // push PaymentIntent message including the error
            if is_custom_error(&error.to_string()) {
              Ok((ws_session_id, PaymentSecret::Err(error.to_string())))
            } else {
              println!("{:?}", error);
              Err(error)
            }
          }
        }?
      },
      CreatePayment::Secondary {..} => {
        let (ws_session_id, buyer_uid, event_id, ticket_type_index, _, _, _, _) = msg.secondary();
        info!("Creating new secondary payment for user {} and ticket {} from event {}", buyer_uid, ticket_type_index, event_id);


        match self.create_secondary_sale_payment(&msg).await {
          Ok(payment_secret) => Ok((ws_session_id, PaymentSecret::Ok(payment_secret))),
          Err(error) => {
            if is_custom_error(&error.to_string()) {
              Ok((ws_session_id, PaymentSecret::Err(error.to_string())))
            } else {
              Err(error)
            }
          }
        }?
      }
    };

    self.store.payment_producer.new_payment(PaymentIntent {
      ws_session_id: ws_session_id.to_string(),
      payment_secret,
    }).await?;

    Ok(())
  }
}
