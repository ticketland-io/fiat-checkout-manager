use borsh::{BorshSerialize, BorshDeserialize};

#[derive(BorshSerialize, BorshDeserialize, PartialEq, Debug)]
pub enum CreatePayment {
  Primary {
    ws_session_id: String,
    buyer_uid: String,
    event_id: String,
    ticket_type_index: u8,
    recipient: String,
  },
  Secondary {
    ws_session_id: String,
    buyer_uid: String,
    event_id: String,
    ticket_type_index: u8,
    recipient: String,
    seat_index: u32,
    cnt_sui_address: String,
    listing_sui_address: String,
  }
}

impl CreatePayment {
  pub fn primary(&self) -> (&str, &str, &str, u8, &str) {
    match self {
      CreatePayment::Primary {
        ws_session_id,
        buyer_uid,
        event_id,
        ticket_type_index,
        recipient,
      } => (
        ws_session_id,
        buyer_uid,
        event_id,
        *ticket_type_index,
        recipient,
      ),
      _ => panic!("should never call primary")
    }
  }

  pub fn secondary(&self) -> (&str, &str, &str, u8, &str, u32, &str, &str) {
    match self {
      CreatePayment::Secondary {
        ws_session_id,
        buyer_uid,
        event_id,
        ticket_type_index,
        recipient,
        seat_index,
        cnt_sui_address,
        listing_sui_address,
      } => (
        ws_session_id,
        buyer_uid,
        event_id,
        *ticket_type_index,
        recipient,
        *seat_index,
        cnt_sui_address,
        listing_sui_address,
      ),
      _ => panic!("should never call primary")
    }
  }
}
