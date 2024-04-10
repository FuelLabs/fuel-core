use super::TransactionsSource;
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
        ImporterDatabase,
    },
    Config,
    Importer,
};
use fuel_core_storage::{
    iter::{
        IterDirection,
        IteratorOverTable,
    },
    tables::{
        merkle::{
            DenseMetadataKey,
            FuelBlockMerkleMetadata,
        },
        FuelBlocks,
    },
    transactional::Changes,
    MerkleRoot,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Consensus,
        SealedBlock,
    },
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

impl BlockImporterAdapter {
    pub fn new(
        chain_id: ChainId,
        config: Config,
        database: Database,
        executor: ExecutorAdapter,
        verifier: VerifierAdapter,
    ) -> Self {
        let importer = Importer::new(chain_id, config, database, executor, verifier);
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

impl ImporterDatabase for Database {
    fn latest_block_height(&self) -> StorageResult<Option<BlockHeight>> {
        Ok(self
            .iter_all::<FuelBlocks>(Some(IterDirection::Reverse))
            .next()
            .transpose()?
            .map(|(height, _)| height))
    }

    fn latest_block_root(&self) -> StorageResult<Option<MerkleRoot>> {
        Ok(self
            .storage_as_ref::<FuelBlockMerkleMetadata>()
            .get(&DenseMetadataKey::Latest)?
            .map(|cow| *cow.root()))
    }
}

impl Executor for ExecutorAdapter {
    fn execute_without_commit(
        &self,
        block: Block,
    ) -> ExecutorResult<UncommittedExecutionResult<Changes>> {
        self._execute_without_commit::<TransactionsSource>(ExecutionTypes::Validation(
            block,
        ))
    }
}
