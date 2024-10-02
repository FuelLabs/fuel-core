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

/// Wrapper around either Transaction or CheckedTransaction.
#[allow(clippy::large_enum_variant)]
pub enum MaybeCheckedTransaction {
    CheckedTransaction(CheckedTransaction, ConsensusParametersVersion),
    Transaction(fuel_tx::Transaction),
}

impl MaybeCheckedTransaction {
    pub fn id(&self, chain_id: &ChainId) -> TxId {
        match self {
            MaybeCheckedTransaction::CheckedTransaction(checked_tx, _) => {
                match checked_tx {
                    // For these variants, we need to pass `chain_id`
                    CheckedTransaction::Script(tx) => tx.as_ref().id(chain_id),
                    CheckedTransaction::Create(tx) => tx.as_ref().id(chain_id),
                    CheckedTransaction::Upgrade(tx) => tx.as_ref().id(chain_id),
                    CheckedTransaction::Upload(tx) => tx.as_ref().id(chain_id),
                    CheckedTransaction::Blob(tx) => tx.as_ref().id(chain_id),
                    // For `Mint`, `id()` does not take `chain_id`
                    CheckedTransaction::Mint(tx) => tx.id(),
                }
            }
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
            Transaction::Upgrade(tx) => Ok(tx.max_gas(gas_costs, fee_params)),
            Transaction::Upload(tx) => Ok(tx.max_gas(gas_costs, fee_params)),
            Transaction::Blob(tx) => Ok(tx.max_gas(gas_costs, fee_params)),
            Transaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have max_gas".to_string(),
            )),
        }
    }
}

impl TransactionExt for CheckedTransaction {
    fn max_gas(&self, _: &ConsensusParameters) -> ExecutorResult<u64> {
        match self {
            CheckedTransaction::Script(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Create(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Upgrade(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Upload(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Blob(tx) => Ok(tx.metadata().max_gas),
            CheckedTransaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have max_gas".to_string(),
            )),
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
    /// Returns the next batch of transactions to satisfy the gas_limit and block_transaction_size_limit.
    /// The returned batch has at most tx_count_limit transactions, none
    /// of which has a size in bytes greater than size_limit.
    fn next(
        &self,
        gas_limit: u64,
        tx_count_limit: u16,
        block_transaction_size_limit: u32,
    ) -> Vec<MaybeCheckedTransaction>;
}

pub trait RelayerPort {
    /// Returns true if the relayer is enabled.
    fn enabled(&self) -> bool;

    /// Get events from the relayer at a given DA height.
    fn get_events(&self, da_height: &DaBlockHeight) -> anyhow::Result<Vec<Event>>;
}
