use std::sync::Arc;
use eyre::{Result, Report};
use ticketland_data::models::ticket_type::SaleType;
use crate::utils::store::Store;

use super::price_feed::get_sui_price;

// 1 unit in Stripe is 100
const STRIPE_UNIT: i64 = 100;

// 1 USDC unit is 1000000
const USDC_UNIT: i64 = 1000000;

/// This is the amount in SOL needed to send a transaction that will mint a new ticket NFT
/// TODO: use the correct value here
const MINT_TICKER_COST_IN_SOL: i64 = 7; // this is 0.007 SOL
const FILL_LISTING_COST_IN_SOL: i64 = 5; // this is 0.006 SOL

// TODO: We consider that all prices are stored with 6 decimals.
// Thus we need to remove 6 decimals from the DB value. This would need
// to be more dynamic in the future. The decimals will be stored in the DB
// record as well
// Note: we first multiply with STRIPE_UNIT to allow for decimals
fn from_usdc_to_stripe_unit(val: i64) -> i64 {
  val * STRIPE_UNIT / USDC_UNIT
}

pub async fn calculate_price_and_fees(
  store: Arc<Store>,
  ticket_price: i64,
  protocol_fee_perc: i64,
  mint_cost: i64
) -> Result<(i64, i64)> {
  let ticket_price = from_usdc_to_stripe_unit(ticket_price);
  let protocol_fee = (ticket_price * protocol_fee_perc) / 10_000;
  let sol_price = get_sui_price(store).await?;
  let mint_cost = (mint_cost * sol_price) / 1000;
  let total_fees = protocol_fee + mint_cost;

  Ok((ticket_price as i64, total_fees as i64))
}


pub enum PrePurchaseChecksParams {
  Primary {
    store: Arc<Store>,
    event_id: String,
    seat_index: u32,
    ticket_type_index: u8,
  },
  Secondary {
    store: Arc<Store>,
    listing_sui_address: String,
    cnt_sui_address: String
  }
}

impl PrePurchaseChecksParams {
  fn primary(self) -> (Arc<Store>, String, u32, u8) {
    match self {
      PrePurchaseChecksParams::Primary {
        store,
        event_id,
        seat_index,
        ticket_type_index,
      } => (store, event_id, seat_index, ticket_type_index),
      _ => panic!("should never call primary")
    }
  }

  fn secondary(self) -> (Arc<Store>, String, String) {
    match self {
      PrePurchaseChecksParams::Secondary {
        store,
        listing_sui_address,
        cnt_sui_address,
      } => (store, listing_sui_address, cnt_sui_address),
      _ => panic!("should never call secondary")
    }
  }
}

pub async fn pre_primary_purchase_checks(params: PrePurchaseChecksParams) -> Result<(i64, i64)> {
  let (store, event_id, seat_index, ticket_type_index) = params.primary();
  
  let mut postgres = store.pg_pool.connection().await?;
  let ticket_type = postgres.read_ticket_type(event_id.to_string(), ticket_type_index as i16).await?;

  // TODO: read object from sui and check existance
  // Using PDA seeds allows us to impose some constraints and do some validation.
  // Ticket nfts are PDAs and part of the seed list is the ticket type index. This allows
  // us to validatate that user does not pass a ticket type which has has lower price but 
  // use a ticket nft that is of a higher, more expensive type.
  // if ticket_nft_pda.to_string() != ticket_nft {
  //   return Err(Report::msg("Invalid ticket_nft"))?
  // }
  // We need to check whether this ticket nft account exists. If it does it means that someone else
  // has already purchased it. We could alternatively load the event_capacity account and check the
  // bit array for availability.
  // let is_ticket_unavailable = store.rpc_client.account_exists(
  //   &Pubkey::from_str(&ticket_nft)?,
  //   CommitmentConfig::processed()
  // ).await?;

  // if is_ticket_unavailable {
  //   return Err(Report::msg("Ticket unavailable"))?
  // }

  if let SaleType::FixedPrice {price} = ticket_type.sale_type {
    calculate_price_and_fees(
      Arc::clone(&store),
      price as i64,
      store.config.ticket_purchase_protocol_fee,
      MINT_TICKER_COST_IN_SOL
    ).await
  } else {
    return Err(Report::msg("Only fixed price ticket types are supported"))?
  }
}

pub async fn pre_secondary_purchase_checks(params: PrePurchaseChecksParams) -> Result<(i64, i64)> {
  let (store, listing_sui_address, cnt_sui_address) = params.secondary();
  let mut postgres = store.pg_pool.connection().await?;
  let listing = postgres.read_listing(listing_sui_address.clone()).await?;

  // Make sure user has send the correct ticket_nft in the request. The provided ticket nft must much the one
  // store in the listing in the db
  if listing.cnt_sui_address != cnt_sui_address {
    return Err(Report::msg("Invalid ticket_nft"))?
  }

  // TODO: read object from sui and check existance
  // We need to check if the listing account exists. If it doesn't then it means that someone has already
  // filled that listing. The program closes listing accounts upon successefull completion.
  // let listing_exists = store.rpc_client.account_exists(
  //   &Pubkey::from_str(&listing_sui_address)?,
  //   CommitmentConfig::processed()
  // ).await?;

  // if listing_exists {
  //   return Err(Report::msg("listing unavailable"))?
  // }

  calculate_price_and_fees(
    Arc::clone(&store),
    listing.ask_price as i64,
    store.config.secondary_market_protocol_fee,
    FILL_LISTING_COST_IN_SOL,
  ).await
}
