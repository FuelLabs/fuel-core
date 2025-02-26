use fuel_core_types::{
    blockchain::{
        header::ConsensusParametersVersion,
        primitives::DaBlockHeight,
    },
    fuel_tx::{
        self,
        field::{
            Expiration,
            Inputs,
        },
        Chargeable,
        ConsensusParameters,
        Input,
        Transaction,
        TxId,
        UniqueIdentifier,
    },
    fuel_types::{
        BlockHeight,
        ChainId,
    },
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
    field::{
        Outputs,
        Witnesses,
    },
    Output,
    Witness,
};

/// The wrapper around either `Transaction` or `CheckedTransaction`.
#[allow(clippy::large_enum_variant)]
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

    pub fn expiration(&self) -> BlockHeight {
        match self {
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Script(tx),
                _,
            ) => tx.transaction().expiration(),
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Create(tx),
                _,
            ) => tx.transaction().expiration(),
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Mint(_),
                _,
            ) => u32::MAX.into(),
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Upgrade(tx),
                _,
            ) => tx.transaction().expiration(),
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Upload(tx),
                _,
            ) => tx.transaction().expiration(),
            MaybeCheckedTransaction::CheckedTransaction(
                CheckedTransaction::Blob(tx),
                _,
            ) => tx.transaction().expiration(),
            MaybeCheckedTransaction::Transaction(Transaction::Script(tx)) => {
                tx.expiration()
            }
            MaybeCheckedTransaction::Transaction(Transaction::Create(tx)) => {
                tx.expiration()
            }
            MaybeCheckedTransaction::Transaction(Transaction::Mint(_)) => u32::MAX.into(),
            MaybeCheckedTransaction::Transaction(Transaction::Upgrade(tx)) => {
                tx.expiration()
            }
            MaybeCheckedTransaction::Transaction(Transaction::Upload(tx)) => {
                tx.expiration()
            }
            MaybeCheckedTransaction::Transaction(Transaction::Blob(tx)) => {
                tx.expiration()
            }
        }
    }
}

pub trait TransactionExt {
    fn inputs(&self) -> ExecutorResult<&Vec<Input>>;

    fn outputs(&self) -> ExecutorResult<&Vec<Output>>;

    fn witnesses(&self) -> ExecutorResult<&Vec<Witness>>;

    fn max_gas(&self, consensus_params: &ConsensusParameters) -> ExecutorResult<u64>;
}

pub trait TransactionWriteExt {
    fn inputs_mut(&mut self) -> ExecutorResult<&mut Vec<Input>>;

    fn outputs_mut(&mut self) -> ExecutorResult<&mut Vec<Output>>;

    fn witnesses_mut(&mut self) -> ExecutorResult<&mut Vec<Witness>>;
}

impl TransactionExt for Transaction {
    fn inputs(&self) -> ExecutorResult<&Vec<Input>> {
        match self {
            Transaction::Script(tx) => Ok(tx.inputs()),
            Transaction::Create(tx) => Ok(tx.inputs()),
            Transaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have inputs".to_string(),
            )),
            Transaction::Upgrade(tx) => Ok(tx.inputs()),
            Transaction::Upload(tx) => Ok(tx.inputs()),
            Transaction::Blob(tx) => Ok(tx.inputs()),
        }
    }

    fn outputs(&self) -> ExecutorResult<&Vec<Output>> {
        match self {
            Transaction::Script(tx) => Ok(tx.outputs()),
            Transaction::Create(tx) => Ok(tx.outputs()),
            Transaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have outputs".to_string(),
            )),
            Transaction::Upgrade(tx) => Ok(tx.outputs()),
            Transaction::Upload(tx) => Ok(tx.outputs()),
            Transaction::Blob(tx) => Ok(tx.outputs()),
        }
    }

    fn witnesses(&self) -> ExecutorResult<&Vec<Witness>> {
        match self {
            Transaction::Script(tx) => Ok(tx.witnesses()),
            Transaction::Create(tx) => Ok(tx.witnesses()),
            Transaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have outputs".to_string(),
            )),
            Transaction::Upgrade(tx) => Ok(tx.witnesses()),
            Transaction::Upload(tx) => Ok(tx.witnesses()),
            Transaction::Blob(tx) => Ok(tx.witnesses()),
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

impl TransactionWriteExt for Transaction {
    fn inputs_mut(&mut self) -> ExecutorResult<&mut Vec<Input>> {
        match self {
            Transaction::Script(tx) => Ok(tx.inputs_mut()),
            Transaction::Create(tx) => Ok(tx.inputs_mut()),
            Transaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have inputs".to_string(),
            )),
            Transaction::Upgrade(tx) => Ok(tx.inputs_mut()),
            Transaction::Upload(tx) => Ok(tx.inputs_mut()),
            Transaction::Blob(tx) => Ok(tx.inputs_mut()),
        }
    }

    fn outputs_mut(&mut self) -> ExecutorResult<&mut Vec<Output>> {
        match self {
            Transaction::Script(tx) => Ok(tx.outputs_mut()),
            Transaction::Create(tx) => Ok(tx.outputs_mut()),
            Transaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have outputs".to_string(),
            )),
            Transaction::Upgrade(tx) => Ok(tx.outputs_mut()),
            Transaction::Upload(tx) => Ok(tx.outputs_mut()),
            Transaction::Blob(tx) => Ok(tx.outputs_mut()),
        }
    }

    fn witnesses_mut(&mut self) -> ExecutorResult<&mut Vec<Witness>> {
        match self {
            Transaction::Script(tx) => Ok(tx.witnesses_mut()),
            Transaction::Create(tx) => Ok(tx.witnesses_mut()),
            Transaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have outputs".to_string(),
            )),
            Transaction::Upgrade(tx) => Ok(tx.witnesses_mut()),
            Transaction::Upload(tx) => Ok(tx.witnesses_mut()),
            Transaction::Blob(tx) => Ok(tx.witnesses_mut()),
        }
    }
}

impl TransactionExt for CheckedTransaction {
    fn inputs(&self) -> ExecutorResult<&Vec<Input>> {
        match self {
            CheckedTransaction::Script(tx) => Ok(tx.transaction().inputs()),
            CheckedTransaction::Create(tx) => Ok(tx.transaction().inputs()),
            CheckedTransaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have inputs".to_string(),
            )),
            CheckedTransaction::Upgrade(tx) => Ok(tx.transaction().inputs()),
            CheckedTransaction::Upload(tx) => Ok(tx.transaction().inputs()),
            CheckedTransaction::Blob(tx) => Ok(tx.transaction().inputs()),
        }
    }

    fn outputs(&self) -> ExecutorResult<&Vec<Output>> {
        match self {
            CheckedTransaction::Script(tx) => Ok(tx.transaction().outputs()),
            CheckedTransaction::Create(tx) => Ok(tx.transaction().outputs()),
            CheckedTransaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have outputs".to_string(),
            )),
            CheckedTransaction::Upgrade(tx) => Ok(tx.transaction().outputs()),
            CheckedTransaction::Upload(tx) => Ok(tx.transaction().outputs()),
            CheckedTransaction::Blob(tx) => Ok(tx.transaction().outputs()),
        }
    }

    fn witnesses(&self) -> ExecutorResult<&Vec<Witness>> {
        match self {
            CheckedTransaction::Script(tx) => Ok(tx.transaction().witnesses()),
            CheckedTransaction::Create(tx) => Ok(tx.transaction().witnesses()),
            CheckedTransaction::Mint(_) => Err(ExecutorError::Other(
                "Mint transaction doesn't have outputs".to_string(),
            )),
            CheckedTransaction::Upgrade(tx) => Ok(tx.transaction().witnesses()),
            CheckedTransaction::Upload(tx) => Ok(tx.transaction().witnesses()),
            CheckedTransaction::Blob(tx) => Ok(tx.transaction().witnesses()),
        }
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

    fn outputs(&self) -> ExecutorResult<&Vec<Output>> {
        match self {
            MaybeCheckedTransaction::CheckedTransaction(tx, _) => tx.outputs(),
            MaybeCheckedTransaction::Transaction(tx) => tx.outputs(),
        }
    }

    fn witnesses(&self) -> ExecutorResult<&Vec<Witness>> {
        match self {
            MaybeCheckedTransaction::CheckedTransaction(tx, _) => tx.witnesses(),
            MaybeCheckedTransaction::Transaction(tx) => tx.witnesses(),
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
        tx_count_limit: u16,
        block_transaction_size_limit: u32,
    ) -> Vec<MaybeCheckedTransaction>;
}

pub trait RelayerPort {
    /// Returns `true` if the relayer is enabled.
    fn enabled(&self) -> bool;

    /// Get events from the relayer at a given da height.
    fn get_events(&self, da_height: &DaBlockHeight) -> anyhow::Result<Vec<Event>>;
}
