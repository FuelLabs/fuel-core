use std::collections::HashSet;

use fuel_core_types::{
    fuel_tx::ContractId,
    fuel_vm::checked_transaction::CheckedTransaction,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionFiltered {
    /// Some transactions were filtered out and so could be fetched in the future
    Filtered,
    /// No transactions were filtered out
    NotFiltered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    /// The set of contract IDs to filter out
    pub excluded_contract_ids: HashSet<ContractId>,
}

impl Filter {
    pub fn new(excluded_contract_ids: HashSet<ContractId>) -> Self {
        Self {
            excluded_contract_ids,
        }
    }
}

pub struct TransactionSourceExecutableTransactions {
    /// The transactions that can be executed
    pub transactions: Vec<CheckedTransaction>,
    /// Indicates whether some transactions were filtered out based on the filter
    pub filtered: TransactionFiltered,
    /// The filter used to fetch these transactions
    pub filter: Filter,
}

pub trait TransactionsSource {
    /// Returns the a batch of transactions to satisfy the given parameters
    fn get_executable_transactions(
        &mut self,
        gas_limit: u64,
        tx_count_limit: u32,
        block_transaction_size_limit: u64,
        filter: Filter,
    ) -> TransactionSourceExecutableTransactions;

    /// Returns a notification receiver for new transactions
    fn get_new_transactions_notifier(&mut self) -> tokio::sync::Notify;
}
