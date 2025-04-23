use crate::{
    database::{
        Database,
        commit_changes_with_height_update,
    },
    service::adapters::{
        BlockImporterAdapter,
        ExecutorAdapter,
        VerifierAdapter,
    },
};
use fuel_core_importer::{
    Config,
    Importer,
    ports::{
        BlockVerifier,
        ImporterDatabase,
        Validator,
    },
};
use fuel_core_storage::{
    MerkleRoot,
    Result as StorageResult,
    StorageAsRef,
    iter::{
        IterDirection,
        IteratorOverTable,
    },
    tables::{
        FuelBlocks,
        merkle::{
            DenseMetadataKey,
            FuelBlockMerkleMetadata,
        },
    },
    transactional::{
        Changes,
        StorageChanges,
    },
};
use fuel_core_txpool::ports::{
    WasmChecker,
    WasmValidityError,
};
use fuel_core_types::{
    blockchain::{
        SealedBlock,
        block::Block,
        consensus::Consensus,
    },
    fuel_tx::Bytes32,
    fuel_types::{
        BlockHeight,
        ChainId,
    },
    services::executor::{
        Result as ExecutorResult,
        UncommittedValidationResult,
    },
};
use itertools::Itertools;
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
        self.iter_all_keys::<FuelBlocks>(Some(IterDirection::Reverse))
            .next()
            .transpose()
    }

    fn latest_block_root(&self) -> StorageResult<Option<MerkleRoot>> {
        Ok(self
            .storage_as_ref::<FuelBlockMerkleMetadata>()
            .get(&DenseMetadataKey::Latest)?
            .map(|cow| *cow.root()))
    }

    fn commit_changes(&mut self, changes: StorageChanges) -> StorageResult<()> {
        commit_changes_with_height_update(self, changes, |iter| {
            iter.iter_all_keys::<FuelBlocks>(Some(IterDirection::Reverse))
                .try_collect()
        })
    }
}

impl Validator for ExecutorAdapter {
    fn validate(
        &self,
        block: &Block,
    ) -> ExecutorResult<UncommittedValidationResult<Changes>> {
        self.executor.validate(block)
    }
}

#[cfg(feature = "wasm-executor")]
impl WasmChecker for ExecutorAdapter {
    fn validate_uploaded_wasm(
        &self,
        wasm_root: &Bytes32,
    ) -> Result<(), WasmValidityError> {
        self.executor
            .validate_uploaded_wasm(wasm_root)
            .map_err(|err| match err {
                fuel_core_upgradable_executor::error::UpgradableError::InvalidWasm(_) => {
                    WasmValidityError::NotValid
                }
                _ => WasmValidityError::NotFound,
            })
    }
}

#[cfg(not(feature = "wasm-executor"))]
impl WasmChecker for ExecutorAdapter {
    fn validate_uploaded_wasm(
        &self,
        _wasm_root: &Bytes32,
    ) -> Result<(), WasmValidityError> {
        Err(WasmValidityError::NotEnabled)
    }
}
