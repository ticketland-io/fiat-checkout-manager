use borsh::{BorshSerialize, BorshDeserialize};

#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug)]
pub struct CheckoutSession {
  pub ws_session_id: String,
  pub checkout_session_od: String,
}
