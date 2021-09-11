use super::Hash;
use fuel_tx::{Bytes32, Transaction};
use fuel_vm::prelude::Word;
use serde::{Deserialize, Serialize};

pub type BlockHeight = u32;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FuelBlock {
    pub id: Hash,
    pub fuel_height: BlockHeight,
    pub transactions: Vec<Bytes32>,
}
