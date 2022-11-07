use std::sync::Arc;
use eyre::Result;
use price_feed::actors::price::get_price_key;
use crate::utils::store::Store;

pub async fn get_sol_price(store: Arc<Store>,) -> Result<i64> {
  let mut redis = store.redis.lock().unwrap();
  let price = redis.get(&get_price_key("solana"))
  .await?
  .parse::<i64>()?;

  Ok(price)
}
