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
    instruction::ReserveSeatIx,
    account_data::SeatReservation
  },
  ticket_nft::pda as ticket_nft_pda,
  secondary_market::{
    self,
    instruction::ReserveSellListingIx,
    account_data::SellListingReservation,
  },
  event_registry::account_data::EventId,
};
use crate::{
  models::{
    create_payment::CreatePayment,
    payment_intent::{PaymentIntent, PaymentSecret},
  },
  utils::store::Store,
  services::stripe::{create_primary_sale_payment, create_secondary_sale_payment},
};

// TODO: We can potentially utilize an external service that will give us the average slot for the last day.
// The Solana target slot time is 400ms but we give 50% margin to that ideal value.
const SOLANA_SLOT_TIME: i64 = 600; // 600 ms

fn is_custom_error(error: &str) -> bool {
  error == "Ticket unavailable"
  || error == "Invalid ticket_nft"
  || error == "Only fixed price ticket types are supported"
  || error == "Sell listing unavailable"
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

  async fn reserve_seat(&self, msg: &CreatePayment, seat_index: u32, seat_name: String,) -> Result<()> {
    let (_, _, sale_account, event_id, _, recipient) = msg.primary();

    let sale = Pubkey::from_str(&sale_account)?;
    let seat_reservation_account = ticket_sale::pda::seat_reservation(&sale, seat_index, &seat_name.to_string()).0;
    let result = self.store.rpc_client.get_anchor_account_data::<SeatReservation>(&seat_reservation_account).await;

    // Fails if the account does not exist
    if result.is_err() {
      return self.send_reserve_seat_tx(
        sale,
        seat_reservation_account,
        event_id.to_string(),
        seat_index,
        seat_name.to_string(),
        recipient.to_string()
      ).await
    }

    let seat_reservation = result?;
    let latest_slot = self.store.rpc_client.get_slot().await?;

    // Ignore if it has expired. Note id recipient is the same recipient as the one we're processing this message for
    // we should still send the reserve seat as this might be a new request for a payment link so we need to
    // update the duration of the reservation which will happen in the reserve_seat Ix.
    if latest_slot > seat_reservation.valid_until {
      return Ok(())
    }

    self.send_reserve_seat_tx(
      sale,
      seat_reservation_account,
      event_id.to_string(),
      seat_index,
      seat_name.to_string(),
      recipient.to_string()
    ).await
  }

  async fn send_reserve_seat_tx(
    &self,
    sale: Pubkey,
    seat_reservation: Pubkey,
    event_id: String,
    seat_index: u32,
    seat_name: String,
    recipient: String,
  ) -> Result<()> {
    let state = self.store.config.ticket_sale_state;
    let operator = self.store.rpc_client.payer_key().context("invalid priv key")?;

    let accounts = vec![
      AccountMeta::new_readonly(state, false),
      AccountMeta::new_readonly(sale, false),
      AccountMeta::new(seat_reservation, false),
      AccountMeta::new(operator, true),
      AccountMeta::new_readonly(system_program::ID, false),
      AccountMeta::new_readonly(Rent::id(), false),
    ];

    let duration = (Duration::minutes(10).num_milliseconds() / SOLANA_SLOT_TIME) as u64;
    let data = ReserveSeatIx {
      seat_index,
      seat_name: seat_name.clone(),
      duration,
      recipient: pubkey_from_str(&recipient)?,
    }.data();

    let ix = Instruction {
      program_id: ticket_sale::program_id(),
      accounts,
      data,
    };

    Ok(
      self.store.rpc_client.send_tx(ix).await
      .map(|tx_hash| println!("Reserved seat {}:{} for event {}: {:?}", seat_index, &seat_name, &event_id, tx_hash))?
    )
  }

  async fn reserve_sell_listing(&self, msg: &CreatePayment) -> Result<()> {
    let (_, _, _, event_id, ticket_nft, _, recipient) = msg.secondary();
    let state = self.store.config.secondary_market_state;
    let ticket_nft_pubkey = Pubkey::from_str(&ticket_nft)?;
    let ticket_matadata = ticket_nft_pda::ticket_metadata(&self.store.config.ticket_nft_state, &ticket_nft_pubkey).0;
    let sell_listing = secondary_market::pda::sell_listing(&state, &event_id, &ticket_matadata,).0;
    let sell_listing_reservation_account = secondary_market::pda::sell_listing_reservation(&sell_listing).0;
    let result = self.store.rpc_client.get_anchor_account_data::<SellListingReservation>(&sell_listing_reservation_account).await;

    // Fails if the account does not exist
    if result.is_err() {
      return self.send_reserve_sell_listing_tx(
        event_id.to_string(),
        sell_listing,
        sell_listing_reservation_account,
        recipient.to_string()
      ).await
    }

    let sell_listing_reservation = result?;
    let latest_slot = self.store.rpc_client.get_slot().await?;

    // Ignore if it has expired. Note id recipient is the same recipient as the one we're processing this message for
    // we should still send the reserve seat as this might be a new request for a payment link so we need to
    // upadte the duration of the reservation which will happen in the reserve_seat Ix.
    let recipient = pubkey_from_str(recipient)?;
    if latest_slot > sell_listing_reservation.valid_until && sell_listing_reservation.recipient != recipient {
      return Ok(())
    }

    self.send_reserve_sell_listing_tx(
      event_id.to_string(),
      sell_listing,
      sell_listing_reservation_account,
      recipient.to_string()
    ).await
    .map(|tx_hash| println!("Reserved fill listing for ticket_nft {} for event {}: {:?}", ticket_nft, &event_id, tx_hash))
  }

  async fn send_reserve_sell_listing_tx(
    &self,
    event_id: String,
    sell_listing: Pubkey,
    sell_listing_reservation: Pubkey,
    recipient: String,
  ) -> Result<()> {
    let state = self.store.config.secondary_market_state;
    let operator = self.store.rpc_client.payer_key().context("invalid priv key")?;

    let accounts = vec![
      AccountMeta::new_readonly(state, false),
      AccountMeta::new(sell_listing_reservation, false),
      AccountMeta::new(operator, true),
      AccountMeta::new_readonly(system_program::ID, false),
      AccountMeta::new_readonly(Rent::id(), false),
    ];

    let duration = (Duration::minutes(5).num_milliseconds() / SOLANA_SLOT_TIME) as u64;
    let data = ReserveSellListingIx {
      sell_listing,
      duration,
      recipient: pubkey_from_str(&recipient)?,
    }.data();

    let ix = Instruction {
      program_id: secondary_market::program_id(),
      accounts,
      data,
    };

    self.store.rpc_client.send_tx(ix)
    .await
    .map(|tx_hash| println!("Reserved sell listing {} for event {}: {:?}", &sell_listing, &event_id, tx_hash))
  }

  async fn create_primary_payment(&self, msg: &CreatePayment, seat_index: u32, seat_name: String, ticket_nft: &Pubkey) -> Result<String> {
    let (_, buyer_uid, sale_account, event_id, ticket_type_index, recipient) = msg.primary();

    Ok(
      create_primary_sale_payment(
        Arc::clone(&self.store),
        buyer_uid.to_string(),
        sale_account.to_string(),
        event_id.to_string(),
        ticket_nft.to_string(),
        ticket_type_index,
        recipient.to_string(),
        seat_index,
        seat_name,
      ).await?
    )
  }

  async fn create_secondary_sale_payment(&self, msg: &CreatePayment) -> Result<String> {
    let (_, buyer_uid, sale_account, event_id, ticket_nft, ticket_type_index, recipient) = msg.secondary();

    Ok(
      create_secondary_sale_payment(
        Arc::clone(&self.store),
        buyer_uid.to_string(),
        sale_account.to_string(),
        event_id.to_string(),
        ticket_nft.to_string(),
        ticket_type_index,
        recipient.to_string(),
      ).await?
    )
  }
}

#[async_trait]
impl Handler<CreatePayment> for CreatePaymentHandler {
  async fn handle(&self, msg: CreatePayment, _: &Delivery, _: i64,) -> Result<()> {
    let (ws_session_id, payment_secret) = match msg {
      CreatePayment::Primary {..} => {
        let (ws_session_id, buyer_uid, _, event_id, ticket_type_index, _,) = msg.primary();

        let seat_index = get_next_seat_index(
          &self.store.pg_pool,
          &self.store.redis_pool,
          Arc::clone(&self.store.rpc_client),
          self.store.config.ticket_sale_state,
          &EventId(event_id.to_string()),
          ticket_type_index
        ).await?;
        let seat_name = seat_index.to_string();

        let ticket_nft = ticket_nft_pda::ticket_nft(
          &self.store.config.ticket_nft_state,
          seat_index,
          &EventId(event_id.to_string()).val(),
          ticket_type_index,
        )
        .0;
        info!("Creating new payment for user {} and ticket {} from event {}", buyer_uid, ticket_nft, event_id);

        with_retry(None, None, || self.reserve_seat(&msg, seat_index, seat_name.clone())).await
        .map_err(|error| {
          println!("Failed to reserve seat {:?}:{:?} for event {}: {:?}", seat_index, seat_name, event_id, error);
          error
        })?;

        match self.create_primary_payment(&msg, seat_index, seat_name.clone(), &ticket_nft).await {
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
        let (ws_session_id, buyer_uid, _, event_id, ticket_nft, _, _) = msg.secondary();
        info!("Creating new secondary payment for user {} and ticket {} from event {}", buyer_uid, ticket_nft, event_id);

        with_retry(None, None, || self.reserve_sell_listing(&msg)).await
        .map_err(|error| {
          println!("Failed to reserve sell listing for ticket_nft {} and event {}: {:?}",  ticket_nft, event_id, error);
          error
        })?;

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
