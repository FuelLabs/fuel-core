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
        ExecutorDatabase,
        ImporterDatabase,
    },
    Config,
    Importer,
};
use fuel_core_storage::{
    iter::IterDirection,
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

impl ImporterDatabase for Database {
    fn latest_block_height(&self) -> StorageResult<Option<BlockHeight>> {
        Ok(self
            .iter_all::<FuelBlocks>(Some(IterDirection::Reverse))
            .next()
            .transpose()?
            .map(|(height, _)| height))
    }
}

impl ExecutorDatabase for Database {
    fn store_new_block(
        &mut self,
        chain_id: &ChainId,
        block: &SealedBlock,
    ) -> StorageResult<bool> {
        let height = block.entity.header().height();
        let mut found = self
            .storage::<FuelBlocks>()
            .insert(height, &block.entity.compress(chain_id))?
            .is_some();
        found |= self
            .storage::<SealedBlockConsensus>()
            .insert(height, &block.consensus)?
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
