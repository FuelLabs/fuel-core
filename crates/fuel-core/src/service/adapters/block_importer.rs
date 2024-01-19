use crate::{
    database::Database,
    service::adapters::{
        BlockImporterAdapter,
        ExecutorAdapter,
        VerifierAdapter,
    },
};
use fuel_core_importer::{
    ports::{
        BlockVerifier,
        Executor,
        ExecutorDatabase,
        ImporterDatabase,
    },
    Config,
    Importer,
};
use fuel_core_poa::ports::RelayerPort;
use fuel_core_storage::{
    tables::{
        FuelBlocks,
        SealedBlockConsensus,
        Transactions,
    },
    transactional::StorageTransaction,
    Result as StorageResult,
    StorageAsMut,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Consensus,
        primitives::DaBlockHeight,
        SealedBlock,
    },
    fuel_tx::UniqueIdentifier,
    fuel_types::{
        BlockHeight,
        ChainId,
    },
    services::executor::{
        ExecutionTypes,
        Result as ExecutorResult,
        UncommittedResult as UncommittedExecutionResult,
    },
};
use std::sync::Arc;

use super::{
    MaybeRelayerAdapter,
    TransactionsSource,
};

impl BlockImporterAdapter {
    pub fn new(
        config: Config,
        database: Database,
        executor: ExecutorAdapter,
        verifier: VerifierAdapter,
    ) -> Self {
        let importer = Importer::new(config, database, executor, verifier);
        importer.init_metrics();
        Self {
            block_importer: Arc::new(importer),
        }
    }

    pub async fn execute_and_commit(
        &self,
        sealed_block: SealedBlock,
    ) -> anyhow::Result<()> {
        self.block_importer.execute_and_commit(sealed_block).await?;
        Ok(())
    }
}

impl BlockVerifier for VerifierAdapter {
    fn verify_block_fields(
        &self,
        consensus: &Consensus,
        block: &Block,
    ) -> anyhow::Result<()> {
        self.block_verifier.verify_block_fields(consensus, block)
    }
}

#[async_trait::async_trait]
impl RelayerPort for MaybeRelayerAdapter {
    async fn await_until_if_in_range(
        &self,
        da_height: &DaBlockHeight,
        _max_da_lag: &DaBlockHeight,
    ) -> anyhow::Result<()> {
        #[cfg(feature = "relayer")]
        {
            if let Some(sync) = self.relayer_synced.as_ref() {
                let current_height = sync.get_finalized_da_height()?;
                anyhow::ensure!(
                    da_height.saturating_sub(*current_height) <= **_max_da_lag,
                    "Relayer is too far out of sync"
                );
                sync.await_at_least_synced(da_height).await?;
            }
            Ok(())
        }
        #[cfg(not(feature = "relayer"))]
        {
            anyhow::ensure!(
                **da_height == 0,
                "Cannot have a da height above zero without a relayer"
            );
            Ok(())
        }
    }
}

impl ImporterDatabase for Database {
    fn latest_block_height(&self) -> StorageResult<Option<BlockHeight>> {
        Ok(self.ids_of_latest_block()?.map(|(height, _)| height))
    }

    fn increase_tx_count(&self, new_txs_count: u64) -> StorageResult<u64> {
        self.increase_tx_count(new_txs_count).map_err(Into::into)
    }
}

impl ExecutorDatabase for Database {
    fn store_new_block(
        &mut self,
        chain_id: &ChainId,
        block: &SealedBlock,
    ) -> StorageResult<bool> {
        let block_id = block.entity.id();
        let mut found = self
            .storage::<FuelBlocks>()
            .insert(&block_id, &block.entity.compress(chain_id))?
            .is_some();
        found |= self
            .storage::<SealedBlockConsensus>()
            .insert(&block_id, &block.consensus)?
            .is_some();

        // TODO: Use `batch_insert` from https://github.com/FuelLabs/fuel-core/pull/1576
        for tx in block.entity.transactions() {
            found |= self
                .storage::<Transactions>()
                .insert(&tx.id(chain_id), tx)?
                .is_some();
        }
        Ok(!found)
    }
}

impl Executor for ExecutorAdapter {
    type Database = Database;

    fn execute_without_commit(
        &self,
        block: Block,
    ) -> ExecutorResult<UncommittedExecutionResult<StorageTransaction<Self::Database>>>
    {
        self._execute_without_commit::<TransactionsSource>(ExecutionTypes::Validation(
            block,
        ))
    }
}
