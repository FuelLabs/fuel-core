use fuel_core_storage::{
    database::{
        MessageIsSpent,
        TxIdOwnerRecorder,
        VmDatabaseTrait,
    },
    tables::{
        Coins,
        ContractsAssets,
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsState,
        FuelBlocks,
        Messages,
        Receipts,
        SpentMessages,
        Transactions,
    },
    transactional::Transactional,
    Error as StorageError,
    MerkleRootStorage,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::fuel_tx::ContractId;

pub trait ExecutorDatabaseTrait<D>:
    VmDatabaseTrait
    + StorageMutate<FuelBlocks, Error = StorageError>
    + StorageMutate<Receipts, Error = StorageError>
    + StorageMutate<Transactions, Error = StorageError>
    + MerkleRootStorage<ContractId, ContractsState>
    + MerkleRootStorage<ContractId, ContractsAssets>
    + StorageInspect<ContractsAssets, Error = StorageError>
    + StorageInspect<ContractsInfo, Error = StorageError>
    + MessageIsSpent<Error = StorageError>
    + StorageMutate<Coins, Error = StorageError>
    + StorageMutate<SpentMessages, Error = StorageError>
    + StorageMutate<ContractsLatestUtxo, Error = StorageError>
    + StorageMutate<Messages, Error = StorageError>
    + StorageMutate<ContractsState, Error = StorageError>
    + Transactional<Storage = D>
    + TxIdOwnerRecorder<Error = StorageError>
{
}
