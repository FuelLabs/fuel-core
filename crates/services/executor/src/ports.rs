use fuel_core_types::{
    blockchain::{
        header::ConsensusParametersVersion,
        primitives::DaBlockHeight,
    },
    fuel_tx::{
        self,
        Chargeable,
        ConsensusParameters,
        Transaction,
        TxId,
        UniqueIdentifier,
    },
    fuel_types::ChainId,
    fuel_vm::checked_transaction::CheckedTransaction,
    services::relayer::Event,
};

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

/// The wrapper around either `Transaction` or `CheckedTransaction`.
pub enum MaybeCheckedTransaction {
    CheckedTransaction(CheckedTransaction, ConsensusParametersVersion),
    Transaction(fuel_tx::Transaction),
}

impl MaybeCheckedTransaction {
    pub fn id(&self, chain_id: &ChainId) -> TxId {
        match self {
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Script(tx),
                _,
            ) => tx.id(),
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Create(tx),
                _,
            ) => tx.id(),
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Mint(tx),
                _,
            ) => tx.id(),
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Upgrade(tx),
                _,
            ) => tx.id(),
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Upload(tx),
                _,
            ) => tx.id(),
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Blob(tx),
                _,
            ) => tx.id(),
            MaybeCheckedTransaction::Transaction(tx) => tx.id(chain_id),
        }
    }

    pub fn max_gas(&self, consensus_params: &ConsensusParameters) -> u64 {
        let fee_params = consensus_params.fee_params();
        let gas_costs = consensus_params.gas_costs();
        match self {
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Script(tx),
                _,
            ) => tx.metadata().max_gas,
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Create(tx),
                _,
            ) => tx.metadata().max_gas,
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Mint(_),
                _,
            ) => 0,
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Upgrade(tx),
                _,
            ) => tx.metadata().max_gas,
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Upload(tx),
                _,
            ) => tx.metadata().max_gas,
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Blob(tx),
                _,
            ) => tx.metadata().max_gas,
            MaybeCheckedTransaction::Transaction(Transaction::Script(tx)) => {
                tx.max_gas(gas_costs, fee_params)
            }
            MaybeCheckedTransaction::Transaction(Transaction::Create(tx)) => {
                tx.max_gas(gas_costs, fee_params)
            }
            MaybeCheckedTransaction::Transaction(Transaction::Mint(_)) => 0,
            MaybeCheckedTransaction::Transaction(Transaction::Upgrade(tx)) => {
                tx.max_gas(gas_costs, fee_params)
            }
            MaybeCheckedTransaction::Transaction(Transaction::Upload(tx)) => {
                tx.max_gas(gas_costs, fee_params)
            }
            MaybeCheckedTransaction::Transaction(Transaction::Blob(tx)) => {
                tx.max_gas(gas_costs, fee_params)
            }
        }
    }
}

pub trait TransactionsSource {
    /// Returns the next batch of transactions to satisfy the `gas_limit`.
    fn next(&self, gas_limit: u64) -> Vec<MaybeCheckedTransaction>;
}

pub trait RelayerPort {
    /// Returns `true` if the relayer is enabled.
    fn enabled(&self) -> bool;

    /// Get events from the relayer at a given da height.
    fn get_events(&self, da_height: &DaBlockHeight) -> anyhow::Result<Vec<Event>>;
}
