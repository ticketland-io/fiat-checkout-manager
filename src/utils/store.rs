use std::sync::Arc;
use sui_sdk::{SuiClientBuilder, SuiClient};
use ticketland_data::connection_pool::ConnectionPool;
use ticketland_core::{
  services::{
    redis, redlock::RedLock,
  },
};
use super::config::Config;
use crate::queue::payment_producer::PaymentProducer;

pub struct Store {
  pub config: Config,
  pub pg_pool: ConnectionPool,
  pub redis_pool: redis::ConnectionPool,
  pub redlock: Arc<RedLock>,
  pub rpc_client: Arc<SuiClient>,
  pub payment_producer: PaymentProducer,
}

impl Store {
  pub async fn new() -> Self {
    let config = Config::new().unwrap();
    let pg_pool = ConnectionPool::new(&config.postgres_uri).await;
    let redis_pool = redis::ConnectionPool::new(&config.redis_host, &config.redis_password, config.redis_port);
    let redlock = Arc::new(RedLock::new(vec![&config.redis_host], &config.redis_password));
    let rpc_client = Arc::new(
      SuiClientBuilder::default()
      .build(&config.sui_rpc)
      .await.unwrap()
    );
    let payment_producer = PaymentProducer::new(
      config.rabbitmq_uri.clone(),
      config.retry_ttl,
    ).await;

    Self {
      config,
      pg_pool,
      redis_pool,
      redlock,
      rpc_client,
      payment_producer,
    }
  }
}
