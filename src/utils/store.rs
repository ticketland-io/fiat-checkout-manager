use std::sync::Arc;
use tokio::sync::Mutex;
use actix::prelude::*;
use ticketland_core::{
  actor::neo4j::Neo4jActor,
  services::{
    redis::Redis,
    redlock::RedLock,
  },
};
use solana_web3_rust::rpc_client::RpcClient;
use super::config::Config;
use crate::queue::payment_producer::PaymentProducer;

pub struct Store {
  pub config: Config,
  pub neo4j: Arc<Addr<Neo4jActor>>,
  pub redis: Arc<Mutex<Redis>>,
  pub redlock: Arc<RedLock>,
  pub rpc_client: Arc<RpcClient>,
  pub payment_producer: PaymentProducer,
}

impl Store {
  pub async fn new() -> Self {
    let config = Config::new().unwrap();
    let neo4j = Arc::new(
      Neo4jActor::new(
        config.neo4j_host.clone(),
        config.neo4j_domain.clone(),
        config.neo4j_username.clone(),
        config.neo4j_password.clone(),
        config.neo4j_database.clone(),
      )
      .await
      .start(),
    );

    let redis = Arc::new(Mutex::new(Redis::new(&config.redis_host, &config.redis_password).await.unwrap()));
    let redlock = Arc::new(RedLock::new(vec![&config.redis_host], &config.redis_password));
    let rpc_client = Arc::new(RpcClient::new(config.rpc_endpoint.clone(), Some(config.operator_priv_key.clone())));

    let payment_producer = PaymentProducer::new(
      config.rabbitmq_uri.clone(),
      config.retry_ttl,
    ).await;

    Self {
      config,
      neo4j,
      redis,
      redlock,
      rpc_client,
      payment_producer,
    }
  }
}
