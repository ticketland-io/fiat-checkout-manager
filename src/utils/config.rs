use std::env;
use solana_sdk::pubkey::Pubkey;
use solana_web3_rust::utils::pubkey_from_str;

pub struct Config {
  pub postgres_uri: String,
  pub rabbitmq_uri: String,
  pub retry_ttl: u32,
  pub redis_host: String,
  pub redis_port: u16,
  pub redis_password: String,
  pub rpc_endpoint: String,
  pub ticketland_dapp: String,
  pub stripe_key: String,
  pub ticket_sale_state: Pubkey,
  pub ticket_nft_state: Pubkey,
  pub secondary_market_state: Pubkey,
  pub ticket_purchase_protocol_fee: i64,
  pub secondary_market_protocol_fee: i64,
  pub operator_priv_key: String,
}

impl Config {
  pub fn new() -> Result<Self, env::VarError> {
    Result::Ok(
      Self {
        postgres_uri: env::var("POSTGRES_URI").unwrap(),
        rabbitmq_uri: env::var("RABBITMQ_URI").unwrap(),
        redis_host: env::var("REDIS_HOST").unwrap(),
        redis_password: env::var("REDIS_PASSWORD").unwrap(),
        redis_port: env::var("REDIS_PORT").unwrap().parse::<u16>().unwrap(),
        rpc_endpoint: env::var("RPC_ENDPOINT").unwrap(),
        ticketland_dapp: env::var("TICKETLAND_DAPP").unwrap(),
        stripe_key: env::var("STRIPE_CLIENT_SECRET").unwrap(),
        ticket_sale_state: pubkey_from_str(&env::var("TICKET_SALE_STATE").unwrap()).unwrap(),
        secondary_market_state: pubkey_from_str(&env::var("SECONDARY_PROGRAM_STATE").unwrap()).unwrap(),
        ticket_purchase_protocol_fee: env::var("TICKET_PURCHASE_PROTOCOL_FEE").unwrap().parse::<i64>().unwrap(),
        ticket_nft_state: pubkey_from_str(&env::var("TICKET_NFT_STATE").unwrap()).unwrap(),
        secondary_market_protocol_fee: env::var("SECONDARY_MARKET_PROTOCOL_FEE").unwrap().parse::<i64>().unwrap(),
        operator_priv_key: env::var("OPERATOR_PRIV_KEY").unwrap(),
        retry_ttl: env::var("RETRY_TTL").unwrap().parse::<u32>().unwrap(),
      }
    )
  }
}
