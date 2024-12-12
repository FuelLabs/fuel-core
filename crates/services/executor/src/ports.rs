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
use fuel_core_types::fuel_tx::{
    field::Inputs,
    Input,
};

/// The wrapper around either `Transaction` or `CheckedTransaction`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
#[cfg_attr(debug_assertions, derive(Clone))]
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

    pub fn is_mint(&self) -> bool {
        match self {
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Mint(_),
                _,
            ) => true,
            MaybeCheckedTransaction::Transaction(Transaction::Mint(_)) => true,
            _ => false,
        }
    }
}

pub trait TransactionExt {
    fn inputs(&self) -> ExecutorResult<&Vec<Input>>;

    fn inputs_mut(&mut self) -> ExecutorResult<&mut Vec<Input>>;

    fn max_gas(&self, consensus_params: &ConsensusParameters) -> ExecutorResult<u64>;
}

impl TransactionExt for Transaction {
    fn inputs(&self) -> ExecutorResult<&Vec<Input>> {
        match self {
            Transaction::Script(tx) => Ok(tx.inputs()),
            Transaction::Create(tx) => Ok(tx.inputs()),
            Transaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have max_gas".to_string(),
            )),
            Transaction::Upgrade(tx) => Ok(tx.inputs()),
            Transaction::Upload(tx) => Ok(tx.inputs()),
            Transaction::Blob(tx) => Ok(tx.inputs()),
        }
    }

    fn inputs_mut(&mut self) -> ExecutorResult<&mut Vec<Input>> {
        match self {
            Transaction::Script(tx) => Ok(tx.inputs_mut()),
            Transaction::Create(tx) => Ok(tx.inputs_mut()),
            Transaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have max_gas".to_string(),
            )),
            Transaction::Upgrade(tx) => Ok(tx.inputs_mut()),
            Transaction::Upload(tx) => Ok(tx.inputs_mut()),
            Transaction::Blob(tx) => Ok(tx.inputs_mut()),
        }
    }

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
    fn inputs(&self) -> ExecutorResult<&Vec<Input>> {
        match self {
            CheckedTransaction::Script(tx) => Ok(tx.transaction().inputs()),
            CheckedTransaction::Create(tx) => Ok(tx.transaction().inputs()),
            CheckedTransaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have max_gas".to_string(),
            )),
            CheckedTransaction::Upgrade(tx) => Ok(tx.transaction().inputs()),
            CheckedTransaction::Upload(tx) => Ok(tx.transaction().inputs()),
            CheckedTransaction::Blob(tx) => Ok(tx.transaction().inputs()),
        }
    }

    fn inputs_mut(&mut self) -> ExecutorResult<&mut Vec<Input>> {
        Err(ExecutorError::Other(
            "It is not allowed to change the `Checked` transaction".to_string(),
        ))
    }

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
    fn inputs(&self) -> ExecutorResult<&Vec<Input>> {
        match self {
            MaybeCheckedTransaction::CheckedTransaction(tx, _) => tx.inputs(),
            MaybeCheckedTransaction::Transaction(tx) => tx.inputs(),
        }
    }

    fn inputs_mut(&mut self) -> ExecutorResult<&mut Vec<Input>> {
        match self {
            MaybeCheckedTransaction::CheckedTransaction(tx, _) => tx.inputs_mut(),
            MaybeCheckedTransaction::Transaction(tx) => tx.inputs_mut(),
        }
    }

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
    /// Returns the next batch of transactions to satisfy the `gas_limit` and `block_transaction_size_limit`.
    /// The returned batch has at most `tx_count_limit` transactions, none
    /// of which has a size in bytes greater than `size_limit`.
    fn next(
        &self,
        gas_limit: u64,
        tx_count_limit: u32,
        block_transaction_size_limit: u32,
    ) -> Vec<MaybeCheckedTransaction>;
}

pub trait RelayerPort {
    /// Returns `true` if the relayer is enabled.
    fn enabled(&self) -> bool;

    /// Get events from the relayer at a given da height.
    fn get_events(&self, da_height: &DaBlockHeight) -> anyhow::Result<Vec<Event>>;
}
