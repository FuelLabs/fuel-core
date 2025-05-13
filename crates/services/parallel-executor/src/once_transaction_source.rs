use std::sync::Mutex;

use fuel_core_executor::ports::{
    MaybeCheckedTransaction,
    TransactionsSource,
};
use fuel_core_types::fuel_vm::checked_transaction::CheckedTransaction;

pub struct OnceTransactionsSource {
    transactions: Mutex<Vec<CheckedTransaction>>,
    consensus_parameters_version: u32,
}

impl OnceTransactionsSource {
    pub fn new(
        transactions: Vec<CheckedTransaction>,
        consensus_parameters_version: u32,
    ) -> Self {
        Self {
            transactions: Mutex::new(transactions),
            consensus_parameters_version,
        }
    }
}

impl TransactionsSource for OnceTransactionsSource {
    fn next(
        &self,
        _gas_limit: u64,
        transactions_limit: u16,
        _block_transaction_size_limit: u32,
    ) -> Vec<fuel_core_executor::ports::MaybeCheckedTransaction> {
        let mut transactions = self.transactions.lock().expect("Mutex poisoned");
        // Avoid panicking if we request more transactions than there are in the vector
        let transactions_limit = (transactions_limit as usize).min(transactions.len());
        transactions
            .drain(..transactions_limit)
            .map(|tx| {
                MaybeCheckedTransaction::CheckedTransaction(
                    tx,
                    self.consensus_parameters_version,
                )
            })
            .collect()
    }
}
