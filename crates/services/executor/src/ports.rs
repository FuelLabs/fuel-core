use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_tx,
    fuel_tx::{
        TxId,
        UniqueIdentifier,
    },
    fuel_types::ChainId,
    fuel_vm::checked_transaction::CheckedTransaction,
    services::relayer::Event,
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
    /// Returns `true` if the relayer is enabled.
    fn enabled(&self) -> bool;

    /// Get events from the relayer at a given da height.
    fn get_events(&self, da_height: &DaBlockHeight) -> anyhow::Result<Vec<Event>>;
}
