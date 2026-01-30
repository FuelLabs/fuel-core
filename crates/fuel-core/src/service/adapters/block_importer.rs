#[cfg(feature = "parallel-executor")]
use crate::service::adapters::{
    ParallelExecutorAdapter,
    ParallelExecutorAdapterInner,
};
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
    Result as StorageResult,
    iter::{
        IterDirection,
        IteratorOverTable,
    },
    tables::FuelBlocks,
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
    #[cfg(not(feature = "parallel-executor"))]
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

    #[cfg(feature = "parallel-executor")]
    pub fn new(
        chain_id: ChainId,
        config: Config,
        database: Database,
        #[cfg(not(feature = "no-parallel-executor"))] executor: ParallelExecutorAdapter,
        #[cfg(feature = "no-parallel-executor")] executor: ExecutorAdapter,
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

#[cfg(feature = "parallel-executor")]
impl Validator for ParallelExecutorAdapter {
    fn validate(
        &self,
        block: &Block,
    ) -> ExecutorResult<UncommittedValidationResult<Changes>> {
        match &self.inner {
            ParallelExecutorAdapterInner::Parallel { .. } => {
                todo!("Implement me please")
            }
            ParallelExecutorAdapterInner::Native(native) => {
                native.executor.validate(block)
            }
        }
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

#[cfg(feature = "wasm-executor")]
#[cfg(feature = "parallel-executor")]
impl WasmChecker for ParallelExecutorAdapter {
    fn validate_uploaded_wasm(
        &self,
        wasm_root: &Bytes32,
    ) -> Result<(), WasmValidityError> {
        match &self.inner {
            ParallelExecutorAdapterInner::Parallel { .. } => {
                unimplemented!("no validation yet")
            }
            ParallelExecutorAdapterInner::Native(native) => native
                .executor
                .validate_uploaded_wasm(wasm_root)
                .map_err(|err| match err {
                    fuel_core_upgradable_executor::error::UpgradableError::InvalidWasm(_) => {
                        WasmValidityError::NotValid
                    }
                    _ => WasmValidityError::NotFound,
                }),
        }
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

#[cfg(not(feature = "wasm-executor"))]
#[cfg(feature = "parallel-executor")]
impl WasmChecker for ParallelExecutorAdapter {
    fn validate_uploaded_wasm(
        &self,
        _wasm_root: &Bytes32,
    ) -> Result<(), WasmValidityError> {
        Err(WasmValidityError::NotEnabled)
    }
}
