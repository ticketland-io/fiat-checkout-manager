use std::sync::Arc;
use ticketland_data::connection_pool::ConnectionPool;
use ticketland_core::{
  services::{
    redis, redlock::RedLock,
  },
};
use solana_web3_rust::rpc_client::RpcClient;
use super::config::Config;
use crate::queue::payment_producer::PaymentProducer;

pub struct Store {
  pub config: Config,
  pub pg_pool: ConnectionPool,
  pub redis_pool: redis::ConnectionPool,
  pub redlock: Arc<RedLock>,
  pub rpc_client: Arc<RpcClient>,
  pub payment_producer: PaymentProducer,
}

impl Store {
  pub async fn new() -> Self {
    let config = Config::new().unwrap();
    let pg_pool = ConnectionPool::new(&config.postgres_uri).await;
    let redis_pool = redis::ConnectionPool::new(&config.redis_host, &config.redis_password, config.redis_port);
    let redlock = Arc::new(RedLock::new(vec![&config.redis_host], &config.redis_password));
    let rpc_client = Arc::new(RpcClient::new(config.rpc_endpoint.clone(), Some(config.operator_priv_key.clone())));

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
