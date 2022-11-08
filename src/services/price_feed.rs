use std::sync::Arc;
use eyre::Result;
use price_feed::actors::price::get_price_key;
use crate::utils::store::Store;

// 1 unit in Stripe is 100
const STRIPE_UNIT: f64 = 100.0;

pub async fn get_sol_price(store: Arc<Store>,) -> Result<i64> {
  let mut redis = store.redis.lock().await;
  let price = redis.get(&get_price_key("solana"))
  .await?
  .parse::<f64>()?;

  Ok((price * STRIPE_UNIT) as i64)
}
