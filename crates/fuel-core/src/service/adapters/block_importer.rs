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
            BlockHeight,
            BlockId,
        },
        SealedBlock,
    },
    services::executor::{
        ExecutionBlock,
        Result as ExecutorResult,
        UncommittedResult as UncommittedExecutionResult,
    },
};
use std::sync::Arc;

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
