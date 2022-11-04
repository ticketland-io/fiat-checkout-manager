use std::env;

pub struct Config {
  pub neo4j_host: String,
  pub neo4j_domain: Option<String>,
  pub neo4j_username: String,
  pub neo4j_password: String,
  pub neo4j_database: Option<String>,
  pub rabbitmq_uri: String,
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
      }
    )
  }
}
