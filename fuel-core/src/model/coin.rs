use crate::model::fuel_block::BlockHeight;
use async_graphql::Enum;
use fuel_asm::Word;
use fuel_tx::{crypto::Hasher, Address, Bytes32, Color};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Copy, Clone, Eq, PartialOrd, PartialEq, Serialize, Deserialize, Enum)]
pub enum CoinStatus {
    Unspent,
    Spent,
}
