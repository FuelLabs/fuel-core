use crate::model::fuel_block::BlockHeight;
use async_graphql::Enum;
use fuel_asm::Word;
use fuel_tx::{crypto::Hasher, Address, Bytes32, Color};
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct UtxoId {
    pub tx_id: Bytes32,
    pub output_index: u8,
}

impl From<UtxoId> for Bytes32 {
    fn from(pointer: UtxoId) -> Self {
        [&pointer.tx_id[..], &pointer.output_index.to_be_bytes()[..]]
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
