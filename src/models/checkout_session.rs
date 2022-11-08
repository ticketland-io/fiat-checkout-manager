use borsh::{BorshSerialize, BorshDeserialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum CheckoutSessionId {
  Ok(String),
  Err(String),
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct CheckoutSession {
  pub ws_session_id: String,
  pub checkout_session_id: CheckoutSessionId,
}
