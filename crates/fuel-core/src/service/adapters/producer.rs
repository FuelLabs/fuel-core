use crate::{
    database::Database,
    service::{
        adapters::{
            BlockProducerAdapter,
            ExecutorAdapter,
            MaybeRelayerAdapter,
            TransactionsSource,
            TxPoolAdapter,
        },
        sub_services::BlockProducerService,
    },
};
use fuel_core_executor::executor::OnceTransactionsSource;
use fuel_core_producer::ports::TxPool;
use fuel_core_storage::{
    not_found,
    tables::FuelBlocks,
    transactional::StorageTransaction,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        primitives,
    },
    fuel_tx,
    fuel_tx::Transaction,
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    services::{
        block_producer::Components,
        executor::{
            ExecutionTypes,
            Result as ExecutorResult,
            TransactionExecutionStatus,
            UncommittedResult,
        },
    },
};
use std::{
    borrow::Cow,
    sync::Arc,
};

impl BlockProducerAdapter {
    pub fn new(block_producer: BlockProducerService) -> Self {
        Self {
            block_producer: Arc::new(block_producer),
        }
    }
}

#[async_trait::async_trait]
impl TxPool for TxPoolAdapter {
    type TxSource = TransactionsSource;

    fn get_source(&self, block_height: BlockHeight) -> Self::TxSource {
        TransactionsSource::new(self.service.clone(), block_height)
    }
}

impl fuel_core_producer::ports::Executor<TransactionsSource> for ExecutorAdapter {
    type Database = Database;

    fn execute_without_commit(
        &self,
        component: Components<TransactionsSource>,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>> {
        self._execute_without_commit(ExecutionTypes::Production(component))
    }
}

impl fuel_core_producer::ports::Executor<Vec<Transaction>> for ExecutorAdapter {
    type Database = Database;

    fn execute_without_commit(
        &self,
        component: Components<Vec<Transaction>>,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>> {
        let Components {
            header_to_produce,
            transactions_source,
            gas_limit,
        } = component;
        self._execute_without_commit(ExecutionTypes::Production(Components {
            header_to_produce,
            transactions_source: OnceTransactionsSource::new(transactions_source),
            gas_limit,
        }))
    }
}

impl fuel_core_producer::ports::DryRunner for ExecutorAdapter {
    fn dry_run(
        &self,
        block: Components<Vec<fuel_tx::Transaction>>,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<TransactionExecutionStatus>> {
        self._dry_run(block, utxo_validation)
    }
}

#[async_trait::async_trait]
impl fuel_core_producer::ports::Relayer for MaybeRelayerAdapter {
    async fn wait_for_at_least(
        &self,
        height: &primitives::DaBlockHeight,
    ) -> anyhow::Result<primitives::DaBlockHeight> {
        #[cfg(feature = "relayer")]
        {
            if let Some(sync) = self.relayer_synced.as_ref() {
                sync.await_at_least_synced(height).await?;
                sync.get_finalized_da_height()
            } else {
                Ok(0u64.into())
            }
        }
        #[cfg(not(feature = "relayer"))]
        {
            anyhow::ensure!(
                **height == 0,
                "Cannot have a da height above zero without a relayer"
            );
            // If the relayer is not enabled, then all blocks are zero.
            Ok(0u64.into())
        }
    }
}

impl fuel_core_producer::ports::BlockProducerDatabase for Database {
    fn get_block(&self, height: &BlockHeight) -> StorageResult<Cow<CompressedBlock>> {
        self.storage::<FuelBlocks>()
            .get(height)?
            .ok_or(not_found!(FuelBlocks))
    }

    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32> {
        self.storage::<FuelBlocks>().root(height).map(Into::into)
    }
}
