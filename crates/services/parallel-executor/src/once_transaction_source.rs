use std::sync::Mutex;

use fuel_core_executor::ports::{
    MaybeCheckedTransaction,
    TransactionsSource as ExecutorTransactionsSource,
};
use fuel_core_types::fuel_vm::checked_transaction::CheckedTransaction;

use crate::ports::{
    TransactionFiltered,
    TransactionSourceExecutableTransactions,
    TransactionsSource,
};

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

impl ExecutorTransactionsSource for OnceTransactionsSource {
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

impl TransactionsSource for OnceTransactionsSource {
    fn get_executable_transactions(
        &mut self,
        _gas_limit: u64,
        tx_count_limit: u16,
        _block_transaction_size_limit: u32,
        filter: crate::ports::Filter,
    ) -> TransactionSourceExecutableTransactions {
        let mut transactions = self.transactions.lock().expect("Mutex poisoned");
        // Avoid panicking if we request more transactions than there are in the vector
        let transactions_limit = (tx_count_limit as usize).min(transactions.len());
        let txs = transactions.drain(..transactions_limit).collect();
        TransactionSourceExecutableTransactions {
            transactions: txs,
            filtered: TransactionFiltered::NotFiltered,
            filter,
        }
    }
    fn get_new_transactions_notifier(&mut self) -> tokio::sync::Notify {
        // This is a one-time source, so we don't need to notify about new transactions
        tokio::sync::Notify::new()
    }
}
