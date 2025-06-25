use std::collections::HashSet;

use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::coins::coin::CompressedCoin,
    fuel_tx::{
        ConsensusParameters,
        ContractId,
        UtxoId,
    },
    fuel_types::BlockHeight,
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
        tx_count_limit: u16,
        block_transaction_size_limit: u32,
        filter: Filter,
    ) -> TransactionSourceExecutableTransactions;

    /// Returns a notification receiver for new transactions
    fn get_new_transactions_notifier(&mut self) -> tokio::sync::Notify;
}

pub trait Storage {
    /// Get a coin by a UTXO
    fn get_coin(&self, utxo: &UtxoId) -> StorageResult<Option<CompressedCoin>>;

    /// Get the DA block height based on provided height
    fn get_da_height_by_l2_height(
        &self,
        block_height: &BlockHeight,
    ) -> StorageResult<Option<DaBlockHeight>>;

    /// Get consensus parameters based on a version
    fn get_consensus_parameters(
        &self,
        consensus_parameters_version: u32,
    ) -> StorageResult<ConsensusParameters>;
}
