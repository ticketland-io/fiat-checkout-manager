use borsh::{BorshSerialize, BorshDeserialize};

#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug)]
pub enum CreateCheckout {
  Primary {
    ws_session_id: String,
    buyer_uid: String,
    sale_account: String,
    event_id: String,
    ticket_nft: String,
    ticket_type_index: u8,
    recipient: String,
  },
  Secondary {
    ws_session_id: String,
    buyer_uid: String,
    sale_account: String,
    event_id: String,
    ticket_nft: String,
    ticket_type_index: u8,
    recipient: String,
    seat_index: u32,
    seat_name: String,
  }
}
