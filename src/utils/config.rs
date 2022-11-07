use std::env;
use solana_sdk::pubkey::Pubkey;
use solana_web3_rust::utils::pubkey_from_str;

pub struct Config {
  pub neo4j_host: String,
  pub neo4j_domain: Option<String>,
  pub neo4j_username: String,
  pub neo4j_password: String,
  pub neo4j_database: Option<String>,
  pub rabbitmq_uri: String,
  pub redis_host: String,
  pub redis_password: String,
  pub rpc_endpoint: String,
  pub ticketland_dapp: String,
  pub stripe_key: String,
  pub ticket_nft_program_state: Pubkey,
  pub secondary_market_state: Pubkey,
  pub ticket_purchase_protocol_fee: i64,
  pub secondary_market_protocol_fee: i64,
}

impl Config {
  pub fn new() -> Result<Self, env::VarError> {
    Result::Ok(
      Self {
        neo4j_host: env::var("NEO4J_HOST").unwrap(),
        neo4j_domain: None,
        neo4j_username: env::var("NEO4J_USERNAME").unwrap(),
        neo4j_password: env::var("NEO4J_PASSWORD").unwrap(),
        neo4j_database: env::var("NEO4J_DATABASE").ok(),
        rabbitmq_uri: env::var("RABBITMQ_URI").unwrap(),
        redis_host: env::var("REDIS_HOST").unwrap(),
        redis_password: env::var("REDIS_PASSWORD").unwrap(),
        rpc_endpoint: env::var("RPC_ENDPOINT").unwrap(),
        ticketland_dapp: env::var("TICKETLAND_DAPP").unwrap(),
        stripe_key: env::var("STRIPE_CLIENT_SECRET").unwrap(),
        ticket_nft_program_state: pubkey_from_str(&env::var("TICKET_NFT_STATE").unwrap()).unwrap(),
        secondary_market_state: pubkey_from_str(&env::var("SECONDARY_PROGRAM_STATE").unwrap()).unwrap(),
        ticket_purchase_protocol_fee: env::var("TICKET_PURCHASE_PROTOCOL_FEE").unwrap().parse::<i64>().unwrap(),
        secondary_market_protocol_fee: env::var("SECONDARY_MARKET_PROTOCOL_FEE").unwrap().parse::<i64>().unwrap(),
      }
    )
  }
}
