use fuel_core_storage::{
    tables::{
        Coins,
        ContractsAssets,
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        Messages,
        ProcessedTransactions,
        SpentMessages,
    },
    transactional::Transactional,
    vm_storage::VmStorageRequirements,
    Error as StorageError,
    MerkleRootStorage,
    StorageMutate,
    StorageRead,
};

use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::Message,
    fuel_tx,
    fuel_tx::{
        TxId,
        UniqueIdentifier,
    },
    fuel_types::{
        ChainId,
        Nonce,
    },
    fuel_vm::checked_transaction::CheckedTransaction,
};

use fuel_core_types::fuel_tx::ContractId;

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

// TODO: Remove `Clone` bound
pub trait ExecutorDatabaseTrait<D>:
    StorageMutate<Messages, Error = StorageError>
    + StorageMutate<ProcessedTransactions, Error = StorageError>
    + MerkleRootStorage<ContractId, ContractsAssets, Error = StorageError>
    + StorageMutate<Coins, Error = StorageError>
    + StorageMutate<SpentMessages, Error = StorageError>
    + StorageMutate<ContractsLatestUtxo, Error = StorageError>
    + StorageMutate<ContractsRawCode, Error = StorageError>
    + StorageRead<ContractsRawCode>
    + StorageMutate<ContractsInfo, Error = StorageError>
    + MerkleRootStorage<ContractId, ContractsState, Error = StorageError>
    + VmStorageRequirements<Error = StorageError>
    + Transactional<Storage = D>
    + Clone
{
}
