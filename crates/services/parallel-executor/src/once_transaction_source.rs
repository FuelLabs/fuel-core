use fuel_core_executor::ports::{
    MaybeCheckedTransaction,
    TransactionsSource as ExecutorTransactionsSource,
};
use std::sync::Mutex;

pub struct OnceTransactionsSource {
    transactions: Mutex<Vec<MaybeCheckedTransaction>>,
}

impl OnceTransactionsSource {
    pub fn new(transactions: Vec<MaybeCheckedTransaction>) -> Self {
        Self {
            transactions: Mutex::new(transactions),
        }
    }
}

impl ExecutorTransactionsSource for OnceTransactionsSource {
    fn next(
        &self,
        _gas_limit: u64,
        transactions_limit: u32,
        _block_transaction_size_limit: u64,
    ) -> Vec<fuel_core_executor::ports::MaybeCheckedTransaction> {
        let mut transactions = self.transactions.lock().expect("Mutex poisoned");
        // Avoid panicking if we request more transactions than there are in the vector
        let transactions_limit = (transactions_limit as usize).min(transactions.len());
        transactions.drain(..transactions_limit).collect()
    }
}
