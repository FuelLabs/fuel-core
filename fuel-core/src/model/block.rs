use super::Hash;
use fuel_tx::Transaction;
use fuel_vm::prelude::Word;

pub type BlockHeight = Word;

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub struct Block {
    pub id: Hash,
    pub height: BlockHeight,
    pub transactions: Vec<Transaction>,
}
