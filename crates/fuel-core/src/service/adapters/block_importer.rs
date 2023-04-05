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
    tables::SealedBlockConsensus,
    transactional::StorageTransaction,
    Result as StorageResult,
    StorageAsMut,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Consensus,
        primitives::{
            BlockId,
            DaBlockHeight,
        },
        SealedBlock,
    },
    fuel_types::BlockHeight,
    services::executor::{
        ExecutionBlock,
        Result as ExecutorResult,
        UncommittedResult as UncommittedExecutionResult,
    },
};
use std::sync::Arc;

use super::MaybeRelayerAdapter;

impl BlockImporterAdapter {
    pub fn new(
        config: Config,
        database: Database,
        executor: ExecutorAdapter,
        verifier: VerifierAdapter,
    ) -> Self {
        Self {
            block_importer: Arc::new(Importer::new(config, database, executor, verifier)),
        }
    }

    pub async fn execute_and_commit(
        &self,
        sealed_block: SealedBlock,
    ) -> anyhow::Result<()> {
        tokio::task::spawn_blocking({
            let importer = self.block_importer.clone();
            move || importer.execute_and_commit(sealed_block)
        })
        .await??;
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
        max_da_lag: &DaBlockHeight,
    ) -> anyhow::Result<()> {
        #[cfg(feature = "relayer")]
        {
            if let Some(sync) = self.relayer_synced.as_ref() {
                let current_height = sync.get_finalized_da_height()?;
                anyhow::ensure!(
                    da_height.saturating_sub(*current_height) <= **max_da_lag,
                    "Relayer is too far out of sync"
                );
                sync.await_at_least_synced(da_height).await?;
            }
            Ok(())
        }
        #[cfg(not(feature = "relayer"))]
        {
            core::mem::drop(max_da_lag);
            anyhow::ensure!(
                **da_height == 0,
                "Cannot have a da height above zero without a relayer"
            );
            Ok(())
        }
    }
}

impl ImporterDatabase for Database {
    fn latest_block_height(&self) -> StorageResult<BlockHeight> {
        self.latest_height()
    }
}

impl ExecutorDatabase for Database {
    fn seal_block(
        &mut self,
        block_id: &BlockId,
        consensus: &Consensus,
    ) -> StorageResult<Option<Consensus>> {
        self.storage::<SealedBlockConsensus>()
            .insert(block_id, consensus)
            .map_err(Into::into)
    }
}

impl Executor for ExecutorAdapter {
    type Database = Database;

    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedExecutionResult<StorageTransaction<Self::Database>>>
    {
        self._execute_without_commit(block)
    }
}
