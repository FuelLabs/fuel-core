use fuel_core_types::{
    fuel_tx::TxId,
    fuel_vm::checked_transaction::CheckedTransaction,
};

use crate::scheduler::SchedulerError;

pub trait CheckedTransactionExt {
    /// Returns the max gas consumed by the transaction
    fn max_gas(&self) -> Result<u64, SchedulerError>;

    /// Return the ID of the transaction
    fn id(&self) -> TxId;
}

impl CheckedTransactionExt for CheckedTransaction {
    fn max_gas(&self) -> Result<u64, SchedulerError> {
        match self {
            CheckedTransaction::Script(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Create(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Mint(_) => Err(SchedulerError::InternalError(
                "mint transaction doesn't have max gas".to_string(),
            )),
            CheckedTransaction::Upgrade(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Upload(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Blob(tx) => Ok(tx.metadata().max_gas),
        }
    }

    fn id(&self) -> TxId {
        match self {
            CheckedTransaction::Script(tx) => tx.id(),
            CheckedTransaction::Create(tx) => tx.id(),
            CheckedTransaction::Mint(tx) => tx.id(),
            CheckedTransaction::Upgrade(tx) => tx.id(),
            CheckedTransaction::Upload(tx) => tx.id(),
            CheckedTransaction::Blob(tx) => tx.id(),
        }
    }
}
