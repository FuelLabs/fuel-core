use std::collections::HashSet;

use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    entities::coins::coin::CompressedCoin,
    fuel_tx::{
        ContractId,
        UtxoId,
    },
};
use fuel_core_upgradable_executor::native_executor::ports::MaybeCheckedTransaction;

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

pub trait TransactionsSource {
    /// Returns the a batch of transactions to satisfy the given parameters
    fn get_executable_transactions(
        &mut self,
        gas_limit: u64,
        tx_count_limit: u16,
        block_transaction_size_limit: u32,
        filter: Filter,
    ) -> (Vec<MaybeCheckedTransaction>, TransactionFiltered);

    /// Returns a notification receiver for new transactions
    fn get_new_transactions_notifier(&mut self) -> tokio::sync::Notify;
}

pub trait Storage {
    /// Get a coin by a UTXO
    fn get_coin(&self, utxo: &UtxoId) -> StorageResult<Option<CompressedCoin>>;
}
