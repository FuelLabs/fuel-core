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
    services::{
        executor::{
            Error as ExecutorError,
            Result as ExecutorResult,
        },
        relayer::Event,
    },
};

#[cfg(feature = "alloc")]
use alloc::{
    string::ToString,
    vec::Vec,
};

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
}

pub trait TransactionExt {
    fn max_gas(&self, consensus_params: &ConsensusParameters) -> ExecutorResult<u64>;
}

impl TransactionExt for Transaction {
    fn max_gas(&self, consensus_params: &ConsensusParameters) -> ExecutorResult<u64> {
        let fee_params = consensus_params.fee_params();
        let gas_costs = consensus_params.gas_costs();
        match self {
            Transaction::Script(tx) => Ok(tx.max_gas(gas_costs, fee_params)),
            Transaction::Create(tx) => Ok(tx.max_gas(gas_costs, fee_params)),
            Transaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have max_gas".to_string(),
            )),
            Transaction::Upgrade(tx) => Ok(tx.max_gas(gas_costs, fee_params)),
            Transaction::Upload(tx) => Ok(tx.max_gas(gas_costs, fee_params)),
            Transaction::Blob(tx) => Ok(tx.max_gas(gas_costs, fee_params)),
        }
    }
}

impl TransactionExt for CheckedTransaction {
    fn max_gas(&self, _: &ConsensusParameters) -> ExecutorResult<u64> {
        match self {
            CheckedTransaction::Script(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Create(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have max_gas".to_string(),
            )),
            CheckedTransaction::Upgrade(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Upload(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Blob(tx) => Ok(tx.metadata().max_gas),
        }
    }
}

impl TransactionExt for MaybeCheckedTransaction {
    fn max_gas(&self, consensus_params: &ConsensusParameters) -> ExecutorResult<u64> {
        match self {
            MaybeCheckedTransaction::CheckedTransaction(tx, _) => {
                tx.max_gas(consensus_params)
            }
            MaybeCheckedTransaction::Transaction(tx) => tx.max_gas(consensus_params),
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
