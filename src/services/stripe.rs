use std::{
  sync::Arc,
  future::Future,
  pin::Pin, str::FromStr,
};
use chrono::{Duration, NaiveDateTime};
use eyre::{Result, Report, ContextCompat};
use stripe::{
  Client, Customer, CreateCustomer,
  Currency, CreatePaymentIntent, Metadata, CreatePaymentIntentTransferData, PaymentIntent, CustomerId
};
use ticketland_core::{
  async_helpers::timeout,
};
use ticketland_data::models::stripe_customer::StripeCustomer;
use ticketland_event_handler::{
  services::ticket_purchase::pending_ticket_key,
};
use crate::utils::store::Store;
use super::ticket_purchase::{
  PrePurchaseChecksParams,
  pre_primary_purchase_checks,
  pre_secondary_purchase_checks,
};

type PrePurchaseCheck = Pin<Box<dyn Future<Output = Result<(i64, i64)>> + Send>>;

pub async fn create_primary_sale_payment(
  store: Arc<Store>,
  buyer_uid: String,
  event_id: String,
  ticket_type_index: u8,
  recipient: String,
  seat_index: u32,
  seat_name: String,
) -> Result<String> {
  let pre_purchase_check_params = PrePurchaseChecksParams::Primary {
    store: Arc::clone(&store),
    event_id: event_id.clone(),
    seat_index,
    ticket_type_index,
  };

  let payment_metadata = Some([
    ("sale_type".to_string(), "primary".to_string()),
    ("buyer_uid".to_string(), buyer_uid.clone()),
    ("event_id".to_string(), event_id.clone()),
    ("ticket_type_index".to_string(), ticket_type_index.to_string()),
    ("recipient".to_string(), recipient.clone()),
    ("seat_index".to_string(), seat_index.to_string()),
    ("seat_name".to_string(), seat_name.clone()),
  ].iter().cloned().collect());

  create_payment(
    store,
    buyer_uid,
    event_id,
    seat_index,
    Box::pin(pre_primary_purchase_checks(pre_purchase_check_params)),
    payment_metadata,
  ).await
}

pub async fn create_secondary_sale_payment(
  store: Arc<Store>,
  buyer_uid: String,
  event_id: String,
  ticket_type_index: u8,
  recipient: String,
  seat_index: u32,
  cnt_sui_address: String,
  listing_sui_address: String,
) -> Result<String> {
  let pre_purchase_check_params = PrePurchaseChecksParams::Secondary {
    store: Arc::clone(&store),
    cnt_sui_address: cnt_sui_address.clone(),
    listing_sui_address: listing_sui_address.to_string(),
  };

  let payment_metadata = Some([
    ("sale_type".to_string(), "secondary".to_string()),
    ("buyer_uid".to_string(), buyer_uid.clone()),
    ("event_id".to_string(), event_id.clone()),
    ("seat_index".to_string(), seat_index.to_string()),
    ("cnt_sui_address".to_string(), cnt_sui_address.clone()),
    ("ticket_type_index".to_string(), ticket_type_index.to_string()),
    ("recipient".to_string(), recipient.clone()),
    ("listing_sui_address".to_string(), listing_sui_address.to_string()),
  ].iter().cloned().collect());

  create_payment(
    store,
    buyer_uid,
    event_id,
    seat_index,
    Box::pin(pre_secondary_purchase_checks(pre_purchase_check_params)),
    payment_metadata,
  ).await
}

pub async fn create_payment(
  store: Arc<Store>,
  buyer_uid: String,
  event_id: String,
  seat_index: u32,
  pre_purchase_checks: PrePurchaseCheck,
  payment_metadata: Option<Metadata>,
) -> Result<String> {
  // There are 5 async calls in this function. Each call will have a time out attached. The total timout is 13 seconds thus
  // this lock will be valid until all calls have successfully processed or until one has a timeout at which point no link is
  // returned to the user and thus the Scenario #3 we describe in the technical documentation will not pose an issue.
  let lock = store.redlock.lock(
    format!("{}:{}", event_id, seat_index).as_bytes(),
    Duration::seconds(15).num_milliseconds() as usize
  ).await?;
  
  // Check if the ticket_nft key is in Redis; If so then the ticket is not available
  // This can happen when someone tries to create a payment session straigth after someone else
  // has already purchased or is in the middle of payment or waiting for the service to send the
  // mint tx to the blockchain.
  let redis_key = pending_ticket_key(&event_id, &seat_index.to_string());
  {
    let mut redis = store.redis_pool.connection().await?;
    if let Ok(_) = redis.get(&redis_key).await {
      return Err(Report::msg("Ticket not available"))
    }
  }

  let (price, fee) = timeout(
    Duration::seconds(5).num_milliseconds() as u64,
    pre_purchase_checks,
  ).await??;

  let client = Client::new(store.config.stripe_key.clone());
  let mut postgres = store.pg_pool.connection().await?;
  let account = postgres.read_account_by_id(buyer_uid.clone()).await?;

  let customer = match postgres.read_stripe_customer(buyer_uid.clone()).await {
    Ok(customer) => customer,
    Err(_) => {
      let descr = buyer_uid.clone();
      let customer = CreateCustomer {
        description: Some(&descr),
        email: account.email.as_ref().map(String::as_str),
        ..Default::default()
      };

      let customer = Customer::create(&client, customer).await?;
      let stripe_customer = StripeCustomer {
        account_id: buyer_uid.clone(),
        customer_uid: customer.id.to_string(),
        created_at: customer.created.map_or(None, |secs| NaiveDateTime::from_timestamp_opt(secs, 0)),
      };

      postgres.upsert_stripe_customer(stripe_customer.clone()).await?;

      stripe_customer
    }
  };

  let stripe_account = postgres.read_event_organizer_stripe_account(event_id.clone()).await?;

  let payment_intent = {
    let mut params = CreatePaymentIntent::new(price, Currency::USD);
    params.customer = CustomerId::from_str(&customer.customer_uid).ok();
    params.application_fee_amount = Some(fee);
    params.transfer_data = Some(CreatePaymentIntentTransferData {
      destination: stripe_account.stripe_uid.clone(),
      ..Default::default()
    });
    params.receipt_email = account.email.as_ref().map(String::as_str);
    params.metadata = payment_metadata;

    println!("params {:?}", params);

    timeout(
      Duration::seconds(2).num_milliseconds() as u64,
      PaymentIntent::create(&client, params),
    ).await??
  };

  // Store ticket nft in Redis to mark it unavailable
  // Add ttl that last one minute longer than the payment duration. This is to avoid some weird
  // race conditions i.e. user checkouts the last second, the entry is removed from redis and another
  // user calls this function at the same time at which point the ticket will not be minted nor the record
  // will be in Redis because it expired and because the payment webhook has not be called yet to insert the
  // entry again into Redis.
  let mut redis = store.redis_pool.connection().await?;
  timeout(
    Duration::seconds(2).num_milliseconds() as u64,
    redis.set_ex(&redis_key, &"1", Duration::minutes(6).num_seconds() as usize),
  ).await??;

  store.redlock.unlock(lock).await;

  let payment_secret = payment_intent.client_secret.context("payment secret not set")?;
  Ok(payment_secret)
}
