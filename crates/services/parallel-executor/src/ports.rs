use fuel_core_executor::ports::MaybeCheckedTransaction;
use fuel_core_types::fuel_tx::ContractId;
use std::collections::HashSet;
use tokio::sync::watch;

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
    pub transactions: Vec<MaybeCheckedTransaction>,
    /// Indicates whether some transactions were filtered out based on the filter
    pub filtered: TransactionFiltered,
    /// The filter used to fetch these transactions
    pub filter: Filter,
}

pub trait TransactionsSource {
    /// Returns the a batch of transactions to satisfy the given parameters
    fn get_executable_transactions(
        &self,
        gas_limit: u64,
        tx_count_limit: u32,
        block_transaction_size_limit: u64,
        filter: Filter,
    ) -> impl Future<Output = anyhow::Result<TransactionSourceExecutableTransactions>>;

    /// Returns a notification receiver for new transactions
    fn get_new_transactions_notifier(&self) -> watch::Receiver<()>;
}
