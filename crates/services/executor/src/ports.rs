use fuel_core_types::{
    blockchain::{
        header::ConsensusParametersVersion,
        primitives::DaBlockHeight,
        transaction::TransactionExt,
    },
    fuel_tx::{
        self,
        ConsensusParameters,
        Input,
        Output,
        Transaction,
        TxId,
        UniqueIdentifier,
        field::Expiration,
    },
    fuel_types::{
        BlockHeight,
        ChainId,
    },
    fuel_vm::checked_transaction::CheckedTransaction,
    services::{
        executor::Result as ExecutorResult,
        preconfirmation::Preconfirmation,
        relayer::Event,
    },
};

use crate::executor::WaitNewTransactionsResult;

#[cfg(all(feature = "alloc", not(feature = "std")))]
use alloc::{
    borrow::Cow,
    vec::Vec,
};

#[cfg(feature = "std")]
use std::borrow::Cow;

use core::future::Future;

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

impl TransactionExt for MaybeCheckedTransaction {
    fn inputs(&self) -> Cow<[Input]> {
        match self {
            MaybeCheckedTransaction::CheckedTransaction(tx, _) => tx.inputs(),
            MaybeCheckedTransaction::Transaction(tx) => tx.inputs(),
        }
    }

    fn outputs(&self) -> Cow<[Output]> {
        match self {
            MaybeCheckedTransaction::CheckedTransaction(tx, _) => tx.outputs(),
            MaybeCheckedTransaction::Transaction(tx) => tx.outputs(),
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

/// Send to the block executor to know if there is new transactions available
/// to include in the block or if we are good to end the block execution.
pub trait NewTxWaiterPort: Send {
    /// Wait for new transactions to be available or timeout.
    fn wait_for_new_transactions(
        &mut self,
    ) -> impl Future<Output = WaitNewTransactionsResult> + Send;
}

/// Sending preconfirmation of transactions executed to other services.
pub trait PreconfirmationSenderPort {
    /// Try to send a batch of pre-confirmations. Will succeed only if
    /// it can be directly sent. Otherwise it will return the batch.
    fn try_send(&self, preconfirmations: Vec<Preconfirmation>) -> Vec<Preconfirmation>;

    /// Send a batch of pre-confirmations, awaiting for the send to be successful.
    fn send(
        &self,
        preconfirmations: Vec<Preconfirmation>,
    ) -> impl Future<Output = ()> + Send;
}
