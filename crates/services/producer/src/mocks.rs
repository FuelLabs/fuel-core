use crate::ports::{
    BinaryMerkleTreeStorage,
    BlockExecutor,
    BlockProducerDatabase,
    Executor,
    Relayer,
    TxPool,
};
use fuel_core_storage::{
    not_found,
    tables::FuelBlockMerkleData,
    transactional::{
        StorageTransaction,
        Transaction,
    },
    Mappable,
    Result as StorageResult,
    StorageAsMut,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        primitives::{
            BlockHeight,
            DaBlockHeight,
        },
    },
    fuel_tx::Receipt,
    fuel_types::{
        Address,
        Bytes32,
    },
    services::{
        executor::{
            Error as ExecutorError,
            ExecutionBlock,
            ExecutionResult,
            Result as ExecutorResult,
            UncommittedResult,
        },
        txpool::ArcPoolTx,
    },
};
use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

// TODO: Replace mocks with `mockall`.

#[derive(Default, Clone)]
pub struct MockRelayer {
    pub block_production_key: Address,
    pub best_finalized_height: DaBlockHeight,
}

#[async_trait::async_trait]
impl Relayer for MockRelayer {
    /// Get the best finalized height from the DA layer
    async fn get_best_finalized_da_height(&self) -> StorageResult<DaBlockHeight> {
        Ok(self.best_finalized_height)
    }
}

#[derive(Default)]
pub struct MockTxPool(pub Vec<ArcPoolTx>);

#[async_trait::async_trait]
impl TxPool for MockTxPool {
    fn get_includable_txs(
        &self,
        _block_height: BlockHeight,
        _max_gas: u64,
    ) -> Vec<ArcPoolTx> {
        self.0.clone().into_iter().collect()
    }
}

#[derive(Default)]
pub struct MockExecutor(pub MockDb);

#[derive(Debug)]
struct DatabaseTransaction {
    database: MockDb,
}

impl Transaction<MockDb> for DatabaseTransaction {
    fn commit(&mut self) -> StorageResult<()> {
        Ok(())
    }
}

impl AsMut<MockDb> for DatabaseTransaction {
    fn as_mut(&mut self) -> &mut MockDb {
        &mut self.database
    }
}

impl AsRef<MockDb> for DatabaseTransaction {
    fn as_ref(&self) -> &MockDb {
        &self.database
    }
}

impl Transaction<MockDb> for MockDb {
    fn commit(&mut self) -> StorageResult<()> {
        Ok(())
    }
}

impl AsMut<MockDb> for MockDb {
    fn as_mut(&mut self) -> &mut MockDb {
        self
    }
}

impl AsRef<MockDb> for MockDb {
    fn as_ref(&self) -> &MockDb {
        self
    }
}

impl Executor<MockDb> for MockExecutor {
    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<MockDb>>> {
        let block = match block {
            ExecutionBlock::Production(block) => block.generate(&[]),
            ExecutionBlock::Validation(block) => block,
        };
        // simulate executor inserting a block
        let mut block_db = self.0.blocks.lock().unwrap();
        block_db.insert(*block.header().height(), block.compress());
        Ok(UncommittedResult::new(
            ExecutionResult {
                block,
                skipped_transactions: vec![],
                tx_status: vec![],
            },
            StorageTransaction::new(self.0.clone()),
        ))
    }

    fn dry_run(
        &self,
        _block: ExecutionBlock,
        _utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<Vec<Receipt>>> {
        Ok(Default::default())
    }
}

pub struct FailingMockExecutor(pub Mutex<Option<ExecutorError>>);

impl Executor<MockDb> for FailingMockExecutor {
    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<MockDb>>> {
        // simulate an execution failure
        let mut err = self.0.lock().unwrap();
        if let Some(err) = err.take() {
            Err(err)
        } else {
            let block = match block {
                ExecutionBlock::Production(b) => b.generate(&[]),
                ExecutionBlock::Validation(b) => b,
            };
            Ok(UncommittedResult::new(
                ExecutionResult {
                    block,
                    skipped_transactions: vec![],
                    tx_status: vec![],
                },
                StorageTransaction::new(MockDb::default()),
            ))
        }
    }

    fn dry_run(
        &self,
        _block: ExecutionBlock,
        _utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<Vec<Receipt>>> {
        let mut err = self.0.lock().unwrap();
        if let Some(err) = err.take() {
            Err(err)
        } else {
            Ok(Default::default())
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct MockDb {
    pub blocks: Arc<Mutex<HashMap<BlockHeight, CompressedBlock>>>,
    pub blocks_by_id: Arc<Mutex<HashMap<BlockId, CompressedBlock>>>,
    pub merklized_blocks: Arc<Mutex<HashMap<u64, Primitive>>>,
    pub messages: Arc<Mutex<HashMap<MessageId, Message>>>,
}

impl BlockProducerDatabase for MockDb {
    fn get_block(&self, height: &BlockHeight) -> StorageResult<Cow<CompressedBlock>> {
        let blocks = self.blocks.lock().unwrap();
        blocks
            .get(height)
            .cloned()
            .map(Cow::Owned)
            .ok_or(not_found!("Didn't find block for test"))
    }

    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32> {
        Ok(Bytes32::new([height.as_usize() as u8; 32]))
    }

    fn current_block_height(&self) -> StorageResult<BlockHeight> {
        let blocks = self.blocks.lock().unwrap();

        Ok(blocks.keys().max().cloned().unwrap_or_default())
    }
}

use fuel_core_storage::{
    tables::FuelBlocks,
    Error as StorageError,
};
use fuel_core_types::{
    blockchain::primitives::BlockId,
    fuel_merkle::{
        binary::{
            MerkleTree,
            Primitive,
        },
        storage::{
            StorageInspect,
            StorageMutate,
        },
    },
};

impl StorageInspect<FuelBlocks> for MockDb {
    type Error = StorageError;

    fn get(&self, key: &BlockId) -> Result<Option<Cow<CompressedBlock>>, Self::Error> {
        let blocks = self.blocks_by_id.lock().unwrap();

        Ok(blocks.get(&key).cloned().map(Cow::Owned))
    }

    fn contains_key(&self, key: &BlockId) -> Result<bool, Self::Error> {
        let blocks = self.blocks_by_id.lock().unwrap();

        Ok(blocks.contains_key(&key))
    }
}

impl StorageMutate<FuelBlocks> for MockDb {
    fn insert(
        &mut self,
        key: &BlockId,
        value: &CompressedBlock,
    ) -> Result<Option<CompressedBlock>, Self::Error> {
        let mut blocks = self.blocks_by_id.lock().unwrap();

        Ok(blocks.insert(*key, value.clone()))
    }

    fn remove(&mut self, key: &BlockId) -> Result<Option<CompressedBlock>, Self::Error> {
        let mut blocks = self.blocks_by_id.lock().unwrap();

        Ok(blocks.remove(&key))
    }
}

impl StorageInspect<FuelBlockMerkleData> for MockDb {
    type Error = StorageError;

    fn get(&self, key: &u64) -> Result<Option<Cow<Primitive>>, Self::Error> {
        let merklized_blocks = self.merklized_blocks.lock().unwrap();

        Ok(merklized_blocks.get(&key).cloned().map(Cow::Owned))
    }

    fn contains_key(&self, key: &u64) -> Result<bool, Self::Error> {
        let merklized_blocks = self.merklized_blocks.lock().unwrap();

        Ok(merklized_blocks.contains_key(key))
    }
}

impl StorageMutate<FuelBlockMerkleData> for MockDb {
    fn insert(
        &mut self,
        key: &u64,
        value: &Primitive,
    ) -> Result<Option<Primitive>, Self::Error> {
        let mut merklized_blocks = self.merklized_blocks.lock().unwrap();

        Ok(merklized_blocks.insert(*key, *value))
    }

    fn remove(&mut self, key: &u64) -> Result<Option<Primitive>, Self::Error> {
        let mut merklized_blocks = self.merklized_blocks.lock().unwrap();

        Ok(merklized_blocks.remove(&key))
    }
}

impl StorageMutate<FuelBlockMerkleData> for &MockDb {
    fn insert(
        &mut self,
        key: &u64,
        value: &Primitive,
    ) -> Result<Option<Primitive>, Self::Error> {
        let mut merklized_blocks = self.merklized_blocks.lock().unwrap();

        Ok(merklized_blocks.insert(*key, *value))
    }

    fn remove(&mut self, key: &u64) -> Result<Option<Primitive>, Self::Error> {
        let mut merklized_blocks = self.merklized_blocks.lock().unwrap();

        Ok(merklized_blocks.remove(&key))
    }
}

impl BinaryMerkleTreeStorage for MockDb {
    fn load_binary_merkle_tree<Table>(
        &self,
        version: u64,
    ) -> Result<MerkleTree<Table, &Self>, StorageError>
    where
        Table: Mappable<Key = u64, SetValue = Primitive, GetValue = Primitive>,
        Self: StorageInspect<Table, Error = StorageError>,
    {
        let tree = MerkleTree::load(self, version).unwrap();
        Ok(tree)
    }
}

impl BlockExecutor for MockDb {
    fn insert_block(
        &mut self,
        block_id: &BlockId,
        block: &CompressedBlock,
    ) -> Result<(), StorageError> {
        self.storage::<FuelBlocks>()
            .insert(block_id, block)
            .unwrap();

        let mut blocks = self.blocks.lock().unwrap();
        blocks.insert(*block.header().height(), block.clone());

        // Get the number of Merklized blocks
        let version = self.merklized_blocks.lock().unwrap().len();
        let mut tree = self
            .load_binary_merkle_tree::<FuelBlockMerkleData>(version as u64)
            .unwrap();
        tree.push(block_id.as_slice()).unwrap();

        Ok(())
    }
}
