use crate::ports::{
    BlockProducerDatabase,
    Executor,
    Relayer,
    TxPool,
};
use fuel_core_storage::{
    not_found,
    transactional::{
        AtomicView,
        StorageTransaction,
        Transaction,
    },
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            CompressedBlock,
        },
        primitives::DaBlockHeight,
    },
    fuel_types::{
        Address,
        BlockHeight,
        Bytes32,
        ChainId,
    },
    services::{
        block_producer::Components,
        executor::{
            Error as ExecutorError,
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
    ops::Deref,
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
    async fn wait_for_at_least(
        &self,
        _: &DaBlockHeight,
    ) -> anyhow::Result<DaBlockHeight> {
        Ok(self.best_finalized_height)
    }
}

#[derive(Default)]
pub struct MockTxPool(pub Vec<ArcPoolTx>);

#[async_trait::async_trait]
impl TxPool for MockTxPool {
    type TxSource = Vec<ArcPoolTx>;

    fn get_source(&self, _: BlockHeight) -> Self::TxSource {
        self.0.clone()
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

fn to_block(component: Components<Vec<ArcPoolTx>>) -> Block {
    let transactions = component
        .transactions_source
        .into_iter()
        .map(|tx| tx.as_ref().into())
        .collect();
    Block::new(component.header_to_produce, transactions, &[])
}

impl Executor<Vec<ArcPoolTx>> for MockExecutor {
    type Database = MockDb;

    fn execute_without_commit(
        &self,
        component: Components<Vec<ArcPoolTx>>,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<MockDb>>> {
        let block = to_block(component);
        // simulate executor inserting a block
        let mut block_db = self.0.blocks.lock().unwrap();
        block_db.insert(
            *block.header().height(),
            block.compress(&ChainId::default()),
        );
        Ok(UncommittedResult::new(
            ExecutionResult {
                block,
                skipped_transactions: vec![],
                tx_status: vec![],
                events: vec![],
            },
            StorageTransaction::new(self.0.clone()),
        ))
    }
}

pub struct FailingMockExecutor(pub Mutex<Option<ExecutorError>>);

impl Executor<Vec<ArcPoolTx>> for FailingMockExecutor {
    type Database = MockDb;

    fn execute_without_commit(
        &self,
        component: Components<Vec<ArcPoolTx>>,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<MockDb>>> {
        // simulate an execution failure
        let mut err = self.0.lock().unwrap();
        if let Some(err) = err.take() {
            Err(err)
        } else {
            let block = to_block(component);
            Ok(UncommittedResult::new(
                ExecutionResult {
                    block,
                    skipped_transactions: vec![],
                    tx_status: vec![],
                    events: vec![],
                },
                StorageTransaction::new(MockDb::default()),
            ))
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct MockDb {
    pub blocks: Arc<Mutex<HashMap<BlockHeight, CompressedBlock>>>,
}

impl AtomicView for MockDb {
    type View = Self;

    type Height = BlockHeight;

    fn latest_height(&self) -> BlockHeight {
        let blocks = self.blocks.lock().unwrap();

        blocks.keys().max().cloned().unwrap_or_default()
    }

    fn view_at(&self, _: &BlockHeight) -> StorageResult<Self::View> {
        Ok(self.latest_view())
    }

    fn latest_view(&self) -> Self::View {
        self.clone()
    }
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
        Ok(Bytes32::new(
            [u8::try_from(*height.deref()).expect("Test use small values"); 32],
        ))
    }
}
