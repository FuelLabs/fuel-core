use super::Hash;
use crate::model::block::BlockHeight;
use fuel_asm::Word;
use fuel_tx::{Address, Color, Hash};

pub type CoinId = Hash;

#[derive(Debug, Copy, Clone)]
pub struct Coin {
    pub id: CoinId,
    pub owner: Address,
    pub amount: Word,
    pub color: Color,
    pub maturity: BlockHeight,
    pub status: CoinStatus,
    pub block_created: BlockHeight,
}

#[derive(Debug, Copy, Clone)]
pub enum CoinStatus {
    Unspent,
    Spent,
}
