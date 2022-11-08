use borsh::{BorshSerialize, BorshDeserialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum PaymentSecret {
  Ok(String),
  Err(String),
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct PaymentIntent {
  pub ws_session_id: String,
  pub payment_secret: PaymentSecret,
}
