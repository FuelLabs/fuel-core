use crate::client::types::scalars::{
    Address,
    HexString,
    Nonce,
};

pub struct Message {
    pub amount: u64,
    pub sender: Address,
    pub recipient: Address,
    pub nonce: Nonce,
    pub data: HexString,
    pub da_height: u64,
}
