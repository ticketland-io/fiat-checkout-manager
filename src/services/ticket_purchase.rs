use std::{
  sync::Arc,
  str::FromStr,
};
use eyre::{Result, Report};
use common_data::{
  helpers::{send_read},
  models::{
    sale::{Sale, SaleType},
    listing::SellListing,
  },
  repositories::{
    sale::read_event_sale,
    listing::read_sell_listing,
  },
};
use program_artifacts::{
  ticket_nft::pda,
};
use solana_sdk::{
  pubkey::Pubkey,
  commitment_config::CommitmentConfig,
};
use crate::utils::store::Store;

use super::price_feed::get_sol_price;

// 1 unit in Stripe is 100
const STRIPE_UNIT: i64 = 100;
const STRIPE_FIXED_FEE: i64 = 30; // 30c
const STRIPE_FEE_PERC: i64 = 29; // 2.9%

/// This is the amount in SOL needed to send a transaction that will mint a new ticket NFT
/// TODO: use the correct value here
const MINT_TICKER_COST_IN_SOL: i64 = 7; // this is 0.007 SOL
const FILL_SELL_LISTING_COST_IN_SOL: i64 = 5; // this is 0.006 SOL

fn to_stripe_unit(val: i64) -> i64 {
  val * STRIPE_UNIT
}

pub async fn calculate_price_and_fees(
  store: Arc<Store>,
  ticket_price: i64,
  protocol_fee_perc: i64,
  mint_cost: i64
) -> Result<(i64, i64)> {
  // TODO: We consider that all prices are stored with 6 decimals.
  // Thus we need to remove 6 decimals from the DB value. This would need
  // to be more dynamic in the future. The decimals will be stored in the DB
  // record as well
  let ticket_price = ticket_price / 1000000;
  let protocol_fee = (ticket_price * protocol_fee_perc) / 10_000;
  let sol_price = to_stripe_unit(get_sol_price(store).await?);
  let mint_cost = (mint_cost * sol_price) / 1000;
  let stripe_fee = (ticket_price * STRIPE_FEE_PERC) / 1000;
  let total_stripe_fees = stripe_fee + STRIPE_FIXED_FEE; // 2.9% + 30c
  let total_fees = protocol_fee + mint_cost + total_stripe_fees;

  Ok((ticket_price as i64, total_fees as i64))
}


pub enum PrePurchaseChecksParams {
  Primary {
    store: Arc<Store>,
    event_id: String,
    seat_index: u32,
    sale_account: String,
    ticket_nft: String
  },
  Secondary {
    store: Arc<Store>,
    sell_listing_account: String,
    ticket_nft: String
  }
}

impl PrePurchaseChecksParams {
  fn primary(self) -> (Arc<Store>, String, u32, String, String) {
    match self {
      PrePurchaseChecksParams::Primary {
        store,
        event_id,
        seat_index,
        sale_account,
        ticket_nft,
      } => (store, event_id, seat_index, sale_account, ticket_nft),
      _ => panic!("should never call primary")
    }
  }

  fn secondary(self) -> (Arc<Store>, String, String) {
    match self {
      PrePurchaseChecksParams::Secondary {
        store,
        sell_listing_account,
        ticket_nft,
      } => (store, sell_listing_account, ticket_nft),
      _ => panic!("should never call secondary")
    }
  }
}

pub async fn pre_primary_purchase_checks(params: PrePurchaseChecksParams) -> Result<(i64, i64)> {
  let (store, event_id, seat_index, sale_account, ticket_nft) = params.primary();
  let ticket_nft_program_state = &store.config.ticket_nft_program_state;
  let (query, db_query_params) = read_event_sale(sale_account.to_string());
  let sale: Sale = send_read(Arc::clone(&store.neo4j), query, db_query_params)
  .await
  .map(TryInto::<Sale>::try_into)??;

  let (ticket_nft_pda, _) = pda::ticket_nft(
    ticket_nft_program_state,
    seat_index,
    &event_id,
    sale.ticket_type_index,
  );

  // Using PDA seeds allows us to impose some constraints and do some validation.
  // Ticket nfts are PDAs and part of the seed list is the ticket type index. This allows
  // us to validatate that user does not pass a ticket type which has has lower price but 
  // use a ticket nft that is of a higher, more expensive type.
  if ticket_nft_pda.to_string() != ticket_nft {
    return Err(Report::msg("Invalid ticket_nft"))?
  }
  // We need to check whether this ticket nft account exists. If it does it means that someone else
  // has already purchased it. We could alternatively load the event_capacity account and check the
  // bit array for availability.
  let is_ticket_unavailable = store.rpc_client.account_exists(
    &Pubkey::from_str(&ticket_nft)?,
    CommitmentConfig::processed()
  ).await?;

  if is_ticket_unavailable {
    return Err(Report::msg("Ticket unavailable"))?
  }

  if let SaleType::FixedPrice {price} = sale.sale_type {
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
  let (store, sell_listing_account, ticket_nft) = params.secondary();
  let (query, db_query_params) = read_sell_listing(sell_listing_account.clone());
  let sell_listing: SellListing = send_read(Arc::clone(&store.neo4j), query, db_query_params)
  .await
  .map(TryInto::<SellListing>::try_into)??;

  // Make sure user has send the correct ticket_nft in the request. The provided ticket nft must much the one
  // store in the sell_listing in the db
  if sell_listing.ticket_nft != ticket_nft {
    return Err(Report::msg("Invalid ticket_nft"))?
  }

  // We need to check if the sell listing account exists. If it doesn't then it means that someone has already
  // filled that sell listing. The program closes sell listing accounts upon successefull completion.
  let sell_listing_exists = store.rpc_client.account_exists(
    &Pubkey::from_str(&sell_listing_account)?,
    CommitmentConfig::processed()
  ).await?;

  if sell_listing_exists {
    return Err(Report::msg("Sell listing unavailable"))?
  }

  calculate_price_and_fees(
    Arc::clone(&store),
    sell_listing.ask_price as i64,
    store.config.secondary_market_protocol_fee,
    FILL_SELL_LISTING_COST_IN_SOL,
  ).await
}
