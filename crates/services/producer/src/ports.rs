use async_trait::async_trait;
use fuel_core_storage::{
    transactional::StorageTransaction,
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        primitives::{
            BlockHeight,
            BlockId,
            DaBlockHeight,
        },
    },
    fuel_merkle::binary::{
        MerkleTree,
        Primitive,
    },
    fuel_tx::Receipt,
    merkle::metadata::DenseMerkleMetadata,
    services::{
        executor::{
            ExecutionBlock,
            Result as ExecutorResult,
            UncommittedResult,
        },
        txpool::ArcPoolTx,
    },
};
use std::borrow::Cow;

pub trait BlockProducerDatabase: Send + Sync {
    /// Fetch previously committed block at given height.
    fn get_block(
        &self,
        fuel_height: BlockHeight,
    ) -> StorageResult<Option<Cow<CompressedBlock>>>;

    /// Fetch the current block height.
    fn current_block_height(&self) -> StorageResult<BlockHeight>;
}

pub trait BlockExecutor {
    fn insert_block(
        &mut self,
        block_id: &BlockId,
        block: &CompressedBlock,
    ) -> Result<(), StorageError>;
}

pub trait BinaryMerkleMetadataStorage {
    fn load_binary_merkle_metadata(
        &self,
        key: &str,
    ) -> Result<DenseMerkleMetadata, StorageError>;
    fn save_binary_merkle_metadata(
        &mut self,
        key: &str,
        metadata: &DenseMerkleMetadata,
    ) -> Result<(), StorageError>;
}

pub trait BinaryMerkleTreeStorage {
    fn load_binary_merkle_tree<Table>(
        &self,
        version: u64,
    ) -> Result<MerkleTree<Table, &Self>, StorageError>
    where
        Table: Mappable<Key = u64, SetValue = Primitive, GetValue = Primitive>,
        Self: StorageInspect<Table, Error = StorageError>;

    fn load_mut_binary_merkle_tree<Table>(
        &mut self,
        version: u64,
    ) -> Result<MerkleTree<Table, &mut Self>, StorageError>
    where
        Table: Mappable<Key = u64, SetValue = Primitive, GetValue = Primitive>,
        Self: StorageMutate<Table, Error = StorageError>;
}

#[async_trait]
pub trait TxPool: Send + Sync {
    fn get_includable_txs(
        &self,
        // could be used by the txpool to filter txs based on maturity
        block_height: BlockHeight,
        // The upper limit for the total amount of gas of these txs
        max_gas: u64,
    ) -> Vec<ArcPoolTx>;
}

#[async_trait::async_trait]
pub trait Relayer: Send + Sync {
    /// Get the best finalized height from the DA layer
    async fn get_best_finalized_da_height(&self) -> StorageResult<DaBlockHeight>;
}

pub trait Executor<Database>: Send + Sync {
    /// Executes the block and returns the result of execution with uncommitted database
    /// transaction.
    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>>;

    /// Executes the block without committing it to the database. During execution collects the
    /// receipts to return them. The `utxo_validation` field can be used to disable the validation
    /// of utxos during execution.
    fn dry_run(
        &self,
        block: ExecutionBlock,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<Vec<Receipt>>>;
}
