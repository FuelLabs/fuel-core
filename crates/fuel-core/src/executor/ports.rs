use fuel_core_txpool::types::TxId;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::Message,
    fuel_tx,
    fuel_tx::UniqueIdentifier,
    fuel_types::{
        ChainId,
        Nonce,
    },
    fuel_vm::checked_transaction::CheckedTransaction,
};

/// The wrapper around either `Transaction` or `CheckedTransaction`.
pub enum MaybeCheckedTransaction {
    CheckedTransaction(CheckedTransaction),
    Transaction(fuel_tx::Transaction),
}

impl MaybeCheckedTransaction {
    pub fn id(&self, chain_id: &ChainId) -> TxId {
        match self {
            MaybeCheckedTransaction::CheckedTransaction(CheckedTransaction::Script(
                tx,
            )) => tx.id(),
            MaybeCheckedTransaction::CheckedTransaction(CheckedTransaction::Create(
                tx,
            )) => tx.id(),
            MaybeCheckedTransaction::CheckedTransaction(CheckedTransaction::Mint(tx)) => {
                tx.id()
            }
            MaybeCheckedTransaction::Transaction(tx) => tx.id(chain_id),
        }
    }
}

pub trait TransactionsSource {
    /// Returns the next batch of transactions to satisfy the `gas_limit`.
    fn next(&self, gas_limit: u64) -> Vec<MaybeCheckedTransaction>;
}

pub trait RelayerPort {
    /// Get a message from the relayer if it has been
    /// synced and is <= the given da height.
    fn get_message(
        &self,
        id: &Nonce,
        da_height: &DaBlockHeight,
    ) -> anyhow::Result<Option<Message>>;
}

#[cfg(test)]
/// For some tests we don't care about the actual
/// implementation of the RelayerPort
/// and using a passthrough is fine.
impl RelayerPort for crate::database::Database {
    fn get_message(
        &self,
        id: &Nonce,
        _da_height: &DaBlockHeight,
    ) -> anyhow::Result<Option<Message>> {
        use fuel_core_storage::{
            tables::Messages,
            StorageAsRef,
        };
        use std::borrow::Cow;
        Ok(self.storage::<Messages>().get(id)?.map(Cow::into_owned))
    }
}
