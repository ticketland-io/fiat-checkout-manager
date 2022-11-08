use std::{
  sync::Arc,
  str::FromStr,
  future::Future,
  pin::Pin,
};
use chrono::{Utc, Duration};
use eyre::{Result, Report};
use serde::{Serialize};
use stripe::{
  Account, AccountLink, AccountLinkType, AccountType, Client, CreateAccount,
  CreateAccountCapabilities, CreateAccountCapabilitiesCardPayments,
  CreateAccountCapabilitiesTransfers, CreateAccountLink, AccountLinkCollect,
  AccountId, AccountSettingsParams, PayoutSettingsParams, TransferScheduleParams,
  TransferScheduleInterval, Customer, CreateCustomer, CreateProduct,
  Product, CreatePrice, Currency, IdOrCreate, Price, CreateCheckoutSession, CheckoutSession,
  CreateCheckoutSessionLineItems, CheckoutSessionMode, CreateCheckoutSessionPaymentIntentData,
  CreateCheckoutSessionPaymentIntentDataTransferData, Metadata,
};
use common_data::{
  helpers::{send_read, send_write},
  models::stripe_account::{StripeAccount},
  repositories::{
    stripe::{
      read_stripe_user,
      upsert_account_link,
      read_event_organizer_stripe_account,
    },
  }
};
use ticketland_core::{
  async_helpers::timeout,
};
use ticketland_event_handler::{
  services::ticket_purchase::pending_ticket_key,
};
use program_artifacts::{
  ticket_nft::pda as ticket_nft_pda,
  secondary_market::pda,
};
use crate::utils::store::Store;
use super::ticket_purchase::{
  PrePurchaseChecksParams,
  pre_primary_purchase_checks,
  pre_secondary_purchase_checks,
};

#[derive(Serialize)]
pub struct Response {
  pub link: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutSessionResponse {
  pub session_id: String,
}

pub async fn create_link(store: Arc<Store>, uid: String) -> Result<String> {
  let neo4j = Arc::clone(&store.neo4j);
  let ticketland_dapp = store.config.ticketland_dapp.clone();
  let uid_copy = uid.clone();

  let stripe_account = create_stripe_account(store.config.stripe_key.clone()).await?;
  let stripe_uid = stripe_account.id.clone();
  let account_link = create_stripe_account_link(
    store.config.stripe_key.clone(),
    stripe_uid.clone(),
    uid_copy.clone(),
    ticketland_dapp,
  ).await?;

  let (query, db_query_params) = upsert_account_link(uid.clone(), stripe_uid.to_string(), account_link.url.clone());
  send_write(Arc::clone(&neo4j), query, db_query_params)
  .await
  .map(|_| account_link.url)
  .map_err(Into::<_>::into)
}

pub async fn refresh_link(store: Arc<Store>, uid: String) -> Result<String> {
  let neo4j = Arc::clone(&store.neo4j);
  let (query, db_query_params) = read_stripe_user(uid.clone());
  let ticketland_dapp = store.config.ticketland_dapp.clone();
  let uid_copy = uid.clone();

  let stripe_account = send_read(Arc::clone(&neo4j), query, db_query_params).await?;
  let stripe_account = TryInto::<StripeAccount>::try_into(stripe_account).unwrap();
  let stripe_uid = stripe_account.stripe_uid.clone();

  let account_link = create_stripe_account_link(
    store.config.stripe_key.clone(),
    AccountId::from_str(&stripe_uid.clone()).unwrap(),
    uid_copy.clone(),
    ticketland_dapp,
  ).await?;

  let (query, db_query_params) = upsert_account_link(uid.clone(), stripe_uid, account_link.url.clone());
      
  send_write(Arc::clone(&neo4j), query, db_query_params)
  .await
  .map(|_| account_link.url.clone())
  .map_err(Into::<_>::into)
}

pub async fn create_stripe_account(secret_key: String,) -> Result<Account>  {
  let client = Client::new(secret_key);
  
  // Do we need to create a manual payout schedule? The reason is that buying a ticket requires two steps.
  // We need to first charge user's card and then send a tx to the blockchain to mint the ticket.
  // However, there are no atomicity guarantees here. For example, we might charge user's card and then realize
  // that the ticket has already been purchased by someone else i.e. race condition. To avoid that we can essentially
  // revert the payment by refunding the original account if something like that happens. In the happy path scenario
  // we would release the payment to the event organizers bank account after a ticket is successfully minted.
  // For more info check https://stripe.com/docs/connect/manual-payouts
  Account::create(
    &client,
    CreateAccount {
      type_: Some(AccountType::Express),
      capabilities: Some(CreateAccountCapabilities {
        card_payments: Some(CreateAccountCapabilitiesCardPayments {
          requested: Some(true),
        }),
        transfers: Some(CreateAccountCapabilitiesTransfers {requested: Some(true)}),
        ..Default::default()
      }),
      settings: Some(AccountSettingsParams {
        payouts: Some(PayoutSettingsParams {
          schedule: Some(TransferScheduleParams {
            interval: Some(TransferScheduleInterval::Daily),
            ..Default::default()
          }),
          ..Default::default()
        }),
        ..Default::default()
      }),
      ..Default::default()
    },
  )
  .await
  .map_err(Into::<_>::into)
}

pub async fn create_stripe_account_link(
  secret_key: String,
  stripe_uid: AccountId,
  uid: String,
  ticketland_dapp: String,
) -> Result<AccountLink> {
  let client = Client::new(secret_key);

  AccountLink::create(
    &client,
    CreateAccountLink {
        account: stripe_uid,
        type_: AccountLinkType::AccountOnboarding,
        collect: Some(AccountLinkCollect::EventuallyDue),
        expand: &[],
        refresh_url: Some(format!("{}/stripe/refresh-url?uid={}", &ticketland_dapp, &uid).as_str()),
        return_url: Some(format!("{}/stripe/return-url", &ticketland_dapp).as_str()),
    },
  )
  .await
  .map_err(Into::<_>::into)
}

type PrePurchaseCheck = Pin<Box<dyn Future<Output = Result<(i64, i64)>> + Send>>;

pub async fn create_primary_sale_checkout(
  store: Arc<Store>,
  buyer_uid: String,
  sale_account: String,
  event_id: String,
  ticket_nft: String,
  ticket_type_index: u8,
  recipient: String,
  seat_index: u32,
  seat_name: String,
) -> Result<String> {
  let pre_purchase_check_params = PrePurchaseChecksParams::Primary {
    store: Arc::clone(&store),
    event_id: event_id.clone(),
    seat_index: seat_index,
    sale_account: sale_account.clone(),
    ticket_nft: ticket_nft.clone(),
  };

  let checkout_metadata = Some([
    ("sale_type".to_string(), "primary".to_string()),
    ("buyer_uid".to_string(), buyer_uid.clone()),
    ("sale_account".to_string(), sale_account.clone()),
    ("event_id".to_string(), event_id.clone()),
    ("ticket_nft".to_string(), ticket_nft.clone()),
    ("ticket_type_index".to_string(), ticket_type_index.to_string()),
    ("recipient".to_string(), recipient.clone()),
    ("seat_index".to_string(), seat_index.to_string()),
    ("seat_name".to_string(), seat_name.clone()),
  ].iter().cloned().collect());

  create_checkout_session(
    store,
    buyer_uid,
    event_id,
    ticket_nft,
    Box::pin(pre_primary_purchase_checks(pre_purchase_check_params)),
    checkout_metadata,
  ).await
}

pub async fn create_secondary_sale_checkout(
  store: Arc<Store>,
  buyer_uid: String,
  sale_account: String,
  event_id: String,
  ticket_nft: String,
  ticket_type_index: u8,
  recipient: String,
) -> Result<String> {
  let ticket_matadata = ticket_nft_pda::ticket_metadata(&store.config.ticket_nft_state, &ticket_nft).0;
  let sell_listing_account = pda::sell_listing(
    &store.config.secondary_market_state,
    &event_id,
    &ticket_matadata,
  ).0;

  let pre_purchase_check_params = PrePurchaseChecksParams::Secondary {
    store: Arc::clone(&store),
    ticket_nft: ticket_nft.clone(),
    sell_listing_account: sell_listing_account.to_string(),
  };

  let checkout_metadata = Some([
    ("sale_type".to_string(), "secondary".to_string()),
    ("buyer_uid".to_string(), buyer_uid.clone()),
    ("sale_account".to_string(), sale_account.clone()),
    ("event_id".to_string(), event_id.clone()),
    ("ticket_nft".to_string(), ticket_nft.clone()),
    ("ticket_type_index".to_string(), ticket_type_index.to_string()),
    ("recipient".to_string(), recipient.clone()),
    ("sell_listing_account".to_string(), sell_listing_account.to_string()),
  ].iter().cloned().collect());

  create_checkout_session(
    store,
    buyer_uid,
    event_id,
    ticket_nft,
    Box::pin(pre_secondary_purchase_checks(pre_purchase_check_params)),
    checkout_metadata,
  ).await
}

pub async fn create_checkout_session(
  store: Arc<Store>,
  buyer_uid: String,
  event_id: String,
  ticket_nft: String,
  pre_purchase_checks: PrePurchaseCheck,
  checkout_metadata: Option<Metadata>,
) -> Result<String> {
  // There are 5 async calls in this function. Each call will have a time out attached. The total timout is 13 seconds thus
  // this lock will be valid until all calls have successfully processed or until one has a timeout at which point no link is
  // returned to the user and thus the Scenario #3 we describe in the technical documentation will not pose an issue.
  let lock = store.redlock.lock(ticket_nft.as_bytes(), Duration::seconds(15).num_milliseconds() as usize).await?;
  
  // Check if the ticket_nft key is in Redis; If so then the ticket is not available
  // This can happen when someone tries to create a checkout session straigth after someone else
  // has already purchased or is in the middle of checkout or waiting for the service to send the
  // mint tx to the blockchain.
  let redis_key = pending_ticket_key(&event_id, &ticket_nft);
  {
    let mut redis = store.redis.lock().await;
    if let Ok(_) = redis.get(&redis_key).await {
      return Err(Report::msg("Ticket not available"))
    }
  }

  let (price, fee) = timeout(
    Duration::seconds(5).num_milliseconds() as u64,
    pre_purchase_checks,
  ).await??;

  let client = Client::new(store.config.stripe_key.clone());
  let neo4j = Arc::clone(&store.neo4j);

  // TODO: we need to add name and email as well. We can read these values from the DB
  let customer = Customer::create(
    &client,
    CreateCustomer {
      description: Some(&buyer_uid.clone()),
      ..Default::default()
    },
  ).await?;

  let product = {
    // TODO: we can additional props to the product such as url name of the event etc.
    let product_name = format!("Ticket {} for event {}", &ticket_nft, &event_id);
    let create_product = CreateProduct::new(&product_name);

    timeout(
      Duration::seconds(2).num_milliseconds() as u64,
      Product::create(&client, create_product),
    ).await??
  };

  let price = {
    // TODO: we might wnat to support multiple currencies
    let mut create_price = CreatePrice::new(Currency::USD);
    create_price.product = Some(IdOrCreate::Id(&product.id));
    create_price.unit_amount = Some(price);
    create_price.expand = &["product"];

    timeout(
      Duration::seconds(2).num_milliseconds() as u64,
      Price::create(&client, create_price),
    ).await??
  };

  let (query, db_query_params) = read_event_organizer_stripe_account(event_id.clone());
  let stripe_account = send_read(Arc::clone(&neo4j), query, db_query_params).await?;
  let stripe_account = TryInto::<StripeAccount>::try_into(stripe_account).unwrap();

  let checkout_session = {
    let ticketland_dapp = store.config.ticketland_dapp.clone();
    // TODO: use the correct urls
    let cancel_url = format!("{}/stripe/cancel", &ticketland_dapp);
    let success_url = format!("{}/stripe/success", &ticketland_dapp);

    let mut params = CreateCheckoutSession::new(&cancel_url, &success_url);
    params.expires_at = Some(Utc::now().timestamp() + Duration::minutes(30).num_seconds());
    params.customer = Some(customer.id);
    params.payment_intent_data = Some(CreateCheckoutSessionPaymentIntentData {
      application_fee_amount: Some(fee),
      transfer_data: Some(CreateCheckoutSessionPaymentIntentDataTransferData {
        destination: stripe_account.stripe_uid,
        ..Default::default()  
      }),
      ..Default::default()
    });

    params.mode = Some(CheckoutSessionMode::Payment);
    params.line_items = Some(vec![CreateCheckoutSessionLineItems {
      quantity: Some(1),
      price: Some(price.id.to_string()),
      ..Default::default()
    }]);
    params.expand = &["line_items", "line_items.data.price.product"];

    // We will use this values in the webhook so we can construct the correct TicketPurchase message that will
    // be further processed by another service.
    params.metadata = checkout_metadata;

    timeout(
      Duration::seconds(2).num_milliseconds() as u64,
      CheckoutSession::create(&client, params),
    ).await??
  };

  // Store ticket nft in Redis to mark it unavailable
  // Add ttl that last one minute longer than the checkout duration. This is to avoid some weird
  // race conditions i.e. user checkouts the last second, the entry is removed from redis and another
  // user calls this function at the same time at which point the ticket will not be minted nor the record
  // will be in Redis because it expired and because the checkout webhook has not be called yet to insert the
  // entry again into Redis.
  let mut redis = store.redis.lock().await;
  timeout(
    Duration::seconds(2).num_milliseconds() as u64,
    redis.set_ex(&redis_key, &"1", Duration::minutes(31).num_milliseconds() as usize),
  ).await??;

  store.redlock.unlock(lock).await;
  Ok(checkout_session.id.to_string())
}
