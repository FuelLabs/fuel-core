use fuel_core_types::fuel_tx::{
    ContractId,
    Transaction,
    field::{
        InputContract,
        MintGasPrice,
    },
};

use crate::scheduler::SchedulerError;

/// Returns the coinbase transaction details if it exists.
pub trait TxCoinbaseExt {
    fn coinbase(&self) -> Result<(u64, ContractId), SchedulerError>;
}

impl TxCoinbaseExt for &[Transaction] {
    fn coinbase(&self) -> Result<(u64, ContractId), SchedulerError> {
        if let Some(Transaction::Mint(mint)) = self.last() {
            Ok((*mint.gas_price(), mint.input_contract().contract_id))
        } else {
            Err(SchedulerError::MintMissing)
        }
    }
}
