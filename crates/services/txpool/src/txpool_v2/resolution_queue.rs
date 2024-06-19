use crate::types::ContractId;
use fuel_core_types::{
    fuel_tx::{
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
};

pub enum ResolutionInput {
    Coin(UtxoId),
    Contract(ContractId),
    Message(Nonce),
}

/// Some transactions may use inputs that have not yet become valid due to network race conditions.
/// We don't want to spend a lot of resources on these transactions until their inputs become
/// valid. The resolution queue stores transactions like this for a short period.
/// If the transaction is not resolved for this short period of time, it is discarded.
pub struct ResolutionQueue {}

impl ResolutionQueue {
    pub fn new() -> Self {
        Self {}
    }

    pub fn contains(&self, tx_id: &TxId) -> bool {
        unimplemented!()
    }
}
