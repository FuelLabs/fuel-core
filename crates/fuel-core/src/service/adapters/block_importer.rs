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
    tables::{
        FuelBlockRoots,
        SealedBlockConsensus,
    },
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
    fuel_tx::Bytes32,
    services::{
        block_importer::UncommittedResult,
        executor::ExecutionBlock,
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
            execution_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        }
    }

    pub async fn execute_and_commit(
        &self,
        sealed_block: SealedBlock,
    ) -> anyhow::Result<()> {
        let permit = self.execution_semaphore.acquire().await?;
        tokio::task::spawn_blocking({
            let importer = self.block_importer.clone();
            move || importer.execute_and_commit(sealed_block)
        })
        .await??;
        core::mem::drop(permit);
        Ok(())
    }
}

impl fuel_core_poa::ports::BlockImporter for BlockImporterAdapter {
    type Database = Database;

    fn commit_result(
        &self,
        result: UncommittedResult<StorageTransaction<Self::Database>>,
    ) -> anyhow::Result<()> {
        self.block_importer
            .commit_result(result)
            .map_err(Into::into)
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

    fn insert_block_header_merkle_root(
        &mut self,
        height: &BlockHeight,
        root: &Bytes32,
    ) -> StorageResult<Option<Bytes32>> {
        self.storage::<FuelBlockRoots>().insert(height, root)
    }
}

impl Executor for ExecutorAdapter {
    type Database = Database;

    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> Result<
        fuel_core_types::services::executor::UncommittedResult<
            StorageTransaction<Self::Database>,
        >,
        fuel_core_types::services::executor::Error,
    > {
        fuel_core_producer::ports::Executor::<Database>::execute_without_commit(
            self, block,
        )
    }
}
