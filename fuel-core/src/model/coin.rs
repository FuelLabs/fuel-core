use super::Hash;
use crate::model::fuel_block::BlockHeight;
use fuel_asm::Word;
use fuel_tx::crypto::Hasher;
use fuel_tx::{Address, Bytes32, Color};
use fuel_vm::data::{Key, Value};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct TxoPointer {
    pub block_height: u32,
    pub tx_index: u32,
    pub output_index: u8,
}

impl From<TxoPointer> for Bytes32 {
    fn from(pointer: TxoPointer) -> Self {
        [
            &pointer.block_height.to_be_bytes()[..],
            &pointer.tx_index.to_be_bytes()[..],
            &pointer.output_index.to_be_bytes()[..],
        ]
        .iter()
        .collect::<Hasher>()
        .digest()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coin {
    pub owner: Address,
    pub amount: Word,
    pub color: Color,
    pub maturity: BlockHeight,
    pub status: CoinStatus,
    pub block_created: BlockHeight,
}

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq, Serialize, Deserialize)]
pub enum CoinStatus {
    Unspent,
    Spent,
}
