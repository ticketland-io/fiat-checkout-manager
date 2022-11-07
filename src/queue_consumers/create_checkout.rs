use std::{
  sync::Arc,
  str::FromStr,
};
use eyre::{Result, ContextCompat};
use tracing::info;
use amqp_helpers::core::types::Handler;
use async_trait::async_trait;
use lapin::{
  message::{Delivery},
};
use solana_sdk::{
  pubkey::Pubkey,
  instruction::{AccountMeta, Instruction},
  system_program,
  rent::Rent,
  sysvar::SysvarId,
};
use ticketland_core::async_helpers::with_retry;
use solana_web3_rust::utils::pubkey_from_str;
use program_artifacts::{
  ix::InstructionData,
  ticket_sale::{
    self,
    account_data::SeatReservation
  },
};
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

  async fn reserve_seat(&self, msg: &CreateCheckout) -> Result<()> {
    let sale = Pubkey::from_str(&msg.sale_account)?;
    let seat_reservation = ticket_sale::pda::seat_reservation(&sale, msg.seat_index, &msg.seat_name).0;
    // Fails if the account does not exist
    let result = self.store.rpc_client.get_anchor_account_data::<SeatReservation>(&seat_reservation).await;

    if result.is_err() {
      return self.send_reserve_seat_tx(msg).await
    }

    let seat_reservation = result?;
    let latest_slot = self.store.rpc_client.get_slot().await?;

    // Ignore if it has expired. Note id recipient is the same recipient as the one we're processing this message for
    // we should still send the reserve seat as this might be a new request for a checkout link so we need to 
    // upadte the duration of the reservation which will happen in the reserve_seat Ix.
    if latest_slot > seat_reservation.valid_until {
      return Ok(())
    }

    self.send_reserve_seat_tx(msg).await
  }

  async fn send_reserve_seat_tx(&self, msg: &CreateCheckout) -> Result<()> {
    let state = self.store.config.ticket_sale_state;
    let sale = Pubkey::from_str(&msg.sale_account)?;
    let seat_reservation = ticket_sale::pda::seat_reservation(&sale, msg.seat_index, &msg.seat_name).0;
    let operator = self.store.rpc_client.payer_key().context("invalid priv key")?;

    let accounts = vec![
      AccountMeta::new(state, false),
      AccountMeta::new(sale, false),
      AccountMeta::new(seat_reservation, false),
      AccountMeta::new(operator, true),
      AccountMeta::new_readonly(system_program::ID, false),
      AccountMeta::new_readonly(Rent::id(), false),
    ];

    // TODO: find the number to Slots that correspond to 30 mints which is the Stripe Checkout duration.
    // We can potentially utilize an external service that will give us the average slot for the last day
    let duration = 10;

    let data = ticket_sale::instruction::ReserveSeat {
      seat_index: msg.seat_index,
      seat_name: msg.seat_name.clone(),
      duration,
      recipient: pubkey_from_str(&msg.recipient)?,
    }.data();
    
    let ix = Instruction {
      program_id: ticket_sale::program_id(),
      accounts,
      data,
    };

    self.store.rpc_client.send_tx(ix)
    .await
    .map(|tx_hash| println!("Reserved seat {}:{} for event {}: {:?}", msg.seat_index, &msg.seat_name, &msg.event_id, tx_hash))
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
    
    with_retry(None, None, || self.reserve_seat(&msg)).await
    .map_err(|error| {
      println!("Failed to reserve seat  {}:{} for event {}: {:?}", msg.seat_index, &msg.seat_name, &msg.event_id, error);
      error
    })?;

    let _checkout_session_id = self.create_checkout_session(&msg).await?;

    // TODO: send checkout_session_id in a message to RabbitMQ
    Ok(())
  }
}
