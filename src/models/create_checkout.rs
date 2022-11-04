use borsh::{BorshSerialize, BorshDeserialize};

#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug)]
pub struct CreateCheckout {
  pub buyer_uid: String,
  pub sale_account: String,
  pub event_id: String,
  pub ticket_nft: String,
  pub ticket_type_index: u8,
  pub recipient: String,
  pub seat_index: u32,
  pub seat_name: String,
}
