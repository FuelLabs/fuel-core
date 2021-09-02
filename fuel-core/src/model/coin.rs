use super::Hash;
use crate::model::fuel_block::BlockHeight;
use fuel_asm::Word;
use fuel_tx::{Address, Bytes32, Color};
use fuel_vm::data::{Key, Value};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct CoinId(pub [u8; 32]);

impl From<CoinId> for Vec<u8> {
    fn from(id: CoinId) -> Self {
        id.0.to_vec()
    }
}

impl From<Bytes32> for CoinId {
    fn from(b32: Bytes32) -> Self {
        Self(b32.deref().clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coin {
    pub id: CoinId,
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

impl Value for Coin {}
