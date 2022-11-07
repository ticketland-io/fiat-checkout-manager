use borsh::{BorshSerialize, BorshDeserialize};

#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug)]
pub struct CreateCheckout {
  pub ws_session_id: String,
  pub buyer_uid: String,
  pub sale_account: String,
  pub event_id: String,
  pub ticket_nft: String,
  pub ticket_type_index: u8,
  pub recipient: String,
  // These two are used in the primary sale checkout not the fill sell listing checkout
  pub seat_index: Option<u32>,
  pub seat_name: Option<String>,
}
