//! Transaction types and trait

use crate::{
    fuel_tx::{
        field::{
            Inputs,
            Outputs,
        },
        Chargeable,
        ConsensusParameters,
        Input,
        Output,
        Transaction,
    },
    fuel_vm::checked_transaction::CheckedTransaction,
    services::executor::{
        Error as ExecutorError,
        Result as ExecutorResult,
    },
};

#[cfg(feature = "alloc")]
use alloc::{
    borrow::Cow,
    string::ToString,
    vec::Vec,
};

/// Extension trait for transactions.
pub trait TransactionExt {
    /// Returns the inputs of the transaction.
    fn inputs(&self) -> Cow<[Input]>;

    /// Returns the outputs of the transaction.
    fn outputs(&self) -> Cow<[Output]>;

    /// Returns the maximum gas of the transaction.
    fn max_gas(&self, consensus_params: &ConsensusParameters) -> ExecutorResult<u64>;
}

impl TransactionExt for Transaction {
    fn inputs(&self) -> Cow<[Input]> {
        match self {
            Transaction::Script(tx) => Cow::Borrowed(tx.inputs()),
            Transaction::Create(tx) => Cow::Borrowed(tx.inputs()),
            Transaction::Mint(_) => Cow::Owned(Vec::new()),
            Transaction::Upgrade(tx) => Cow::Borrowed(tx.inputs()),
            Transaction::Upload(tx) => Cow::Borrowed(tx.inputs()),
            Transaction::Blob(tx) => Cow::Borrowed(tx.inputs()),
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

    fn outputs(&self) -> Cow<[Output]> {
        match self {
            Transaction::Script(tx) => Cow::Borrowed(tx.outputs()),
            Transaction::Create(tx) => Cow::Borrowed(tx.outputs()),
            Transaction::Mint(_) => Cow::Owned(Vec::new()),
            Transaction::Upgrade(tx) => Cow::Borrowed(tx.outputs()),
            Transaction::Upload(tx) => Cow::Borrowed(tx.outputs()),
            Transaction::Blob(tx) => Cow::Borrowed(tx.outputs()),
        }
    }
}

impl TransactionExt for CheckedTransaction {
    fn inputs(&self) -> Cow<[Input]> {
        match self {
            CheckedTransaction::Script(tx) => Cow::Borrowed(tx.transaction().inputs()),
            CheckedTransaction::Create(tx) => Cow::Borrowed(tx.transaction().inputs()),
            CheckedTransaction::Mint(_) => Cow::Owned(Vec::new()),
            CheckedTransaction::Upgrade(tx) => Cow::Borrowed(tx.transaction().inputs()),
            CheckedTransaction::Upload(tx) => Cow::Borrowed(tx.transaction().inputs()),
            CheckedTransaction::Blob(tx) => Cow::Borrowed(tx.transaction().inputs()),
        }
    }

    fn outputs(&self) -> Cow<[Output]> {
        match self {
            CheckedTransaction::Script(tx) => Cow::Borrowed(tx.transaction().outputs()),
            CheckedTransaction::Create(tx) => Cow::Borrowed(tx.transaction().outputs()),
            CheckedTransaction::Mint(_) => Cow::Owned(Vec::new()),
            CheckedTransaction::Upgrade(tx) => Cow::Borrowed(tx.transaction().outputs()),
            CheckedTransaction::Upload(tx) => Cow::Borrowed(tx.transaction().outputs()),
            CheckedTransaction::Blob(tx) => Cow::Borrowed(tx.transaction().outputs()),
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
