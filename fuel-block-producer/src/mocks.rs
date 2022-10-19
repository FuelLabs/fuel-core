use super::{
    db::BlockProducerDatabase,
    ports::Relayer,
};
use crate::ports::TxPool;
use anyhow::Result;
use async_trait::async_trait;
use fuel_core_interfaces::{
    common::{
        fuel_storage::StorageInspect,
        fuel_tx::{
            CheckedTransaction,
            MessageId,
        },
        fuel_types::Address,
    },
    db::{
        KvStoreError,
        Messages,
    },
    executor::{
        Error as ExecutorError,
        ExecutionBlock,
        Executor,
    },
    model::{
        BlockHeight,
        DaBlockHeight,
        FuelBlock,
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

impl Relayer for MockRelayer {
    /// Get the best finalized height from the DA layer
    fn get_best_finalized_da_height(&self) -> Result<DaBlockHeight> {
        Ok(self.best_finalized_height)
    }
}

#[derive(Default)]
pub struct MockTxPool(pub Vec<CheckedTransaction>);

#[async_trait::async_trait]
impl TxPool for MockTxPool {
    async fn get_includable_txs(
        &self,
        _block_height: BlockHeight,
        _max_gas: u64,
    ) -> Result<Vec<Arc<CheckedTransaction>>> {
        Ok(self.0.iter().cloned().map(Arc::new).collect())
    }
}

#[derive(Default)]
pub struct MockExecutor(pub MockDb);

#[async_trait]
impl Executor for MockExecutor {
    async fn execute(&self, block: ExecutionBlock) -> Result<FuelBlock, ExecutorError> {
        let block = match block {
            ExecutionBlock::Production(block) => block.generate(&[]),
            ExecutionBlock::Validation(block) => block,
        };
        // simulate executor inserting a block
        let mut block_db = self.0.blocks.lock().unwrap();
        block_db.insert(*block.header().height(), block.to_db_block());
        Ok(block)
    }
}

pub struct FailingMockExecutor(pub Mutex<Option<ExecutorError>>);

#[async_trait]
impl Executor for FailingMockExecutor {
    async fn execute(&self, block: ExecutionBlock) -> Result<FuelBlock, ExecutorError> {
        // simulate an execution failure
        let mut err = self.0.lock().unwrap();
        if let Some(err) = err.take() {
            Err(err)
        } else {
            match block {
                ExecutionBlock::Production(b) => Ok(b.generate(&[])),
                ExecutionBlock::Validation(b) => Ok(b),
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct MockDb {
    pub blocks: Arc<Mutex<HashMap<BlockHeight, FuelBlockDb>>>,
    pub messages: Arc<Mutex<HashMap<MessageId, Message>>>,
}

impl StorageInspect<Messages> for MockDb {
    type Error = KvStoreError;

    fn get(
        &self,
        key: &MessageId,
    ) -> std::result::Result<Option<Cow<Message>>, Self::Error> {
        let messages = self.messages.lock().unwrap();
        Ok(messages.get(key).cloned().map(Cow::Owned))
    }

    fn contains_key(&self, key: &MessageId) -> std::result::Result<bool, Self::Error> {
        let messages = self.messages.lock().unwrap();
        Ok(messages.contains_key(key))
    }
}

impl BlockProducerDatabase for MockDb {
    /// fetch previously committed block at given height
    fn get_block(&self, fuel_height: BlockHeight) -> Result<Option<Cow<FuelBlockDb>>> {
        let blocks = self.blocks.lock().unwrap();

        Ok(blocks.get(&fuel_height).cloned().map(Cow::Owned))
    }
}
