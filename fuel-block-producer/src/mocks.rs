use crate::ports::{
    BlockProducerDatabase,
    DBTransaction,
    Executor,
    Relayer,
    TxPool,
};
use anyhow::Result;
use fuel_core_interfaces::{
    common::{
        fuel_tx::{
            MessageId,
            Receipt,
        },
        fuel_types::Address,
    },
    db::{
        Error,
        Transactional,
    },
    executor::{
        Error as ExecutorError,
        ExecutionBlock,
        ExecutionResult,
        UncommittedResult,
    },
    model::{
        ArcPoolTx,
        BlockHeight,
        DaBlockHeight,
        FuelBlockDb,
        Message,
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

#[derive(Default, Clone)]
pub struct MockRelayer {
    pub block_production_key: Address,
    pub best_finalized_height: DaBlockHeight,
}

#[async_trait::async_trait]
impl Relayer for MockRelayer {
    /// Get the best finalized height from the DA layer
    async fn get_best_finalized_da_height(&self) -> Result<DaBlockHeight> {
        Ok(self.best_finalized_height)
    }
}

#[derive(Default)]
pub struct MockTxPool(pub Vec<ArcPoolTx>);

#[async_trait::async_trait]
impl TxPool for MockTxPool {
    async fn get_includable_txs(
        &self,
        _block_height: BlockHeight,
        _max_gas: u64,
    ) -> Result<Vec<ArcPoolTx>> {
        Ok(self.0.clone().into_iter().collect())
    }
}

#[derive(Default)]
pub struct MockExecutor(pub MockDb);

#[derive(Debug)]
struct DatabaseTransaction {
    database: MockDb,
}

impl Transactional for DatabaseTransaction {
    fn commit(self) -> Result<(), Error> {
        Ok(())
    }

    fn commit_box(self: Box<Self>) -> Result<(), Error> {
        Ok(())
    }
}

impl fuel_core_interfaces::db::DatabaseTransaction<MockDb> for DatabaseTransaction {
    fn database(&self) -> &MockDb {
        &self.database
    }

    fn database_mut(&mut self) -> &mut MockDb {
        &mut self.database
    }
}

impl Executor<MockDb> for MockExecutor {
    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> Result<UncommittedResult<DBTransaction<MockDb>>, ExecutorError> {
        let block = match block {
            ExecutionBlock::Production(block) => block.generate(&[]),
            ExecutionBlock::Validation(block) => block,
        };
        // simulate executor inserting a block
        let mut block_db = self.0.blocks.lock().unwrap();
        block_db.insert(*block.header().height(), block.to_db_block());
        Ok(UncommittedResult::new(
            ExecutionResult {
                block,
                skipped_transactions: vec![],
                tx_status: vec![],
            },
            Box::new(DatabaseTransaction {
                database: self.0.clone(),
            }),
        ))
    }

    fn dry_run(
        &self,
        _block: ExecutionBlock,
        _utxo_validation: Option<bool>,
    ) -> std::result::Result<Vec<Vec<Receipt>>, ExecutorError> {
        Ok(Default::default())
    }
}

pub struct FailingMockExecutor(pub Mutex<Option<ExecutorError>>);

impl Executor<MockDb> for FailingMockExecutor {
    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> Result<UncommittedResult<DBTransaction<MockDb>>, ExecutorError> {
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
                Box::new(DatabaseTransaction {
                    database: MockDb::default(),
                }),
            ))
        }
    }

    fn dry_run(
        &self,
        _block: ExecutionBlock,
        _utxo_validation: Option<bool>,
    ) -> std::result::Result<Vec<Vec<Receipt>>, ExecutorError> {
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
    pub blocks: Arc<Mutex<HashMap<BlockHeight, FuelBlockDb>>>,
    pub messages: Arc<Mutex<HashMap<MessageId, Message>>>,
}

impl BlockProducerDatabase for MockDb {
    fn get_block(&self, fuel_height: BlockHeight) -> Result<Option<Cow<FuelBlockDb>>> {
        let blocks = self.blocks.lock().unwrap();

        Ok(blocks.get(&fuel_height).cloned().map(Cow::Owned))
    }

    fn current_block_height(&self) -> Result<BlockHeight> {
        let blocks = self.blocks.lock().unwrap();

        Ok(blocks.keys().max().cloned().unwrap_or_default())
    }
}
