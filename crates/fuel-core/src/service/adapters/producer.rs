use crate::{
    database::Database,
    service::adapters::{
        BlockProducerAdapter,
        ExecutorAdapter,
        MaybeRelayerAdapter,
        TxPoolAdapter,
    },
};
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
        primitives::{
            self,
            BlockHeight,
        },
    },
    fuel_tx::Receipt,
    fuel_types::Bytes32,
    services::{
        executor::{
            ExecutionBlock,
            Result as ExecutorResult,
            UncommittedResult,
        },
        txpool::ArcPoolTx,
    },
};
use std::{
    borrow::Cow,
    sync::Arc,
};

impl BlockProducerAdapter {
    pub fn new(block_producer: fuel_core_producer::Producer<Database>) -> Self {
        Self {
            block_producer: Arc::new(block_producer),
        }
    }
}

#[async_trait::async_trait]
impl TxPool for TxPoolAdapter {
    fn get_includable_txs(
        &self,
        _block_height: BlockHeight,
        max_gas: u64,
    ) -> Vec<ArcPoolTx> {
        self.service.select_transactions(max_gas)
    }
}

#[async_trait::async_trait]
impl fuel_core_producer::ports::Executor<Database> for ExecutorAdapter {
    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>> {
        self._execute_without_commit(block)
    }

    fn dry_run(
        &self,
        block: ExecutionBlock,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<Vec<Receipt>>> {
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
            use fuel_core_relayer::ports::RelayerDb;
            if let Some(sync) = self.relayer_synced.as_ref() {
                sync.await_at_least_synced(height).await?;
            }

            Ok(self.database.get_finalized_da_height().unwrap_or_default())
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
        let id = self.get_block_id(height)?.ok_or(not_found!("BlockId"))?;
        self.storage::<FuelBlocks>()
            .get(&id)?
            .ok_or(not_found!(FuelBlocks))
    }

    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32> {
        self.storage::<FuelBlocks>().root(height).map(Into::into)
    }

    fn current_block_height(&self) -> StorageResult<BlockHeight> {
        self.latest_height()
    }
}
