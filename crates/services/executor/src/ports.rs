use fuel_core_storage::{
    tables::{
        Coins,
        ContractsAssets,
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        FuelBlocks,
        Messages,
        Receipts,
        SpentMessages,
        Transactions,
    },
    transactional::Transactional,
    vm_storage::VmStorageRequirements,
    Error as StorageError,
    MerkleRootStorage,
    StorageInspect,
    StorageMutate,
    StorageRead,
};

use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::Message,
    fuel_tx,
    fuel_tx::{
        Address,
        Bytes32,
        TxId,
        UniqueIdentifier,
    },
    fuel_types::{
        BlockHeight,
        ChainId,
        Nonce,
    },
    fuel_vm::checked_transaction::CheckedTransaction,
    services::txpool::TransactionStatus,
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

pub trait MessageIsSpent:
    StorageInspect<SpentMessages, Error = StorageError>
    + StorageInspect<Messages, Error = StorageError>
{
    type Error;

    fn message_is_spent(&self, nonce: &Nonce) -> Result<bool, StorageError>;
}

pub trait TxIdOwnerRecorder {
    type Error;

    fn record_tx_id_owner(
        &self,
        owner: &Address,
        block_height: BlockHeight,
        tx_idx: u16,
        tx_id: &Bytes32,
    ) -> Result<Option<Bytes32>, Self::Error>;

    fn update_tx_status(
        &self,
        id: &Bytes32,
        status: TransactionStatus,
    ) -> Result<Option<TransactionStatus>, Self::Error>;
}

// TODO: Remove `Clone` bound
pub trait ExecutorDatabaseTrait<D>:
    StorageMutate<FuelBlocks, Error = StorageError>
    + StorageMutate<Receipts, Error = StorageError>
    + StorageMutate<Transactions, Error = StorageError>
    + MerkleRootStorage<ContractId, ContractsAssets>
    + StorageInspect<ContractsAssets, Error = StorageError>
    + MessageIsSpent<Error = StorageError>
    + StorageMutate<Coins, Error = StorageError>
    + StorageMutate<SpentMessages, Error = StorageError>
    + StorageMutate<ContractsLatestUtxo, Error = StorageError>
    + StorageMutate<Messages, Error = StorageError>
    + StorageMutate<ContractsRawCode, Error = StorageError>
    + StorageRead<ContractsRawCode, Error = StorageError>
    + StorageMutate<ContractsInfo, Error = StorageError>
    + MerkleRootStorage<ContractId, ContractsState, Error = StorageError>
    + VmStorageRequirements<Error = StorageError>
    + Transactional<Storage = D>
    + TxIdOwnerRecorder<Error = StorageError>
    + Clone
{
}
