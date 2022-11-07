use borsh,

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
    seat_index: u32,
    seat_name: String,
  },
  Secondary {
    ws_session_id: String,
    buyer_uid: String,
    sale_account: String,
    event_id: String,
    ticket_nft: String,
    ticket_type_index: u8,
    recipient: String,
  }
}

impl CreateCheckout {
  fn primary(self) -> (String, String, String, String, String, u8, String, u32, String) {
    match self {
      CreateCheckout::Primary {
        ws_session_id,
        buyer_uid,
        sale_account,
        event_id,
        ticket_nft,
        ticket_type_index,
        recipient,
        seat_index,
        seat_name,
      } => (ws_session_id, buyer_uid, sale_account, event_id, ticket_nft, ticket_type_index, recipient, seat_index, seat_name),
      _ => panic!("should never call primary")
    }
  }

  fn secondary(self) -> (String, String, String, String, String, u8, String) {
    match self {
      CreateCheckout::Secondary {
        ws_session_id,
        buyer_uid,
        sale_account,
        event_id,
        ticket_nft,
        ticket_type_index,
        recipient,
      } => (ws_session_id, buyer_uid, sale_account, event_id, ticket_nft, ticket_type_index, recipient),
      _ => panic!("should never call primary")
    }
  }
}
