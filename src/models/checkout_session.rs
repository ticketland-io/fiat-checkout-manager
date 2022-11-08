use borsh::{BorshSerialize, BorshDeserialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum Status {
  Ok,
  Err(String),
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct CheckoutSession {
  pub status: Status,
  pub ws_session_id: String,
  pub checkout_session_id: Option<String>,
}
