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
        let database_for_height = database.clone();
        let importer =
            Importer::new(chain_id, config, database, executor, verifier.clone());
        Self {
            #[cfg(feature = "relayer")]
            fcr_relayer: Arc::new(std::sync::OnceLock::new()),
            #[cfg(feature = "relayer")]
            fcr_verifier: verifier,
            block_importer: Arc::new(importer),
            database: database_for_height,
        }
    }

    pub async fn execute_and_commit(
        &self,
        sealed_block: SealedBlock,
    ) -> anyhow::Result<()> {
        #[cfg(feature = "relayer")]
        self.check_fcr_candidate(&sealed_block)?;
        self.block_importer.execute_and_commit(sealed_block).await?;
        Ok(())
    }
    #[cfg(feature = "relayer")]
    fn check_fcr_candidate(&self, block: &SealedBlock) -> anyhow::Result<()> {
        use fuel_core_types::fuel_merkle::binary::root_calculator::MerkleRootCalculator;
        let Some(adapter) = self.fcr_relayer.get() else {
            return Ok(());
        };
        let Some(shared) = &adapter.relayer_synced else {
            return Ok(());
        };
        let status = shared.fcr_status();
        if !status.enabled {
            return Ok(());
        }
        anyhow::ensure!(
            status.state != "initializing",
            "Canonical safety audit pending"
        );
        anyhow::ensure!(!shared.fcr_halted(), "Canonical Ethereum finality incident");
        // Sample the *published* frontier before reading events. Local catch-up in a
        // concurrent task must never turn a lag failure into a divergence accusation.
        let published = shared.get_finalized_da_height();
        let candidate = block.entity.header();
        anyhow::ensure!(
            candidate.da_height() <= published,
            "Local DA catch-up pending"
        );
        let sealed_header = fuel_core_types::blockchain::SealedBlockHeader {
            entity: candidate.clone(),
            consensus: block.consensus.clone(),
        };
        if !self
            .fcr_verifier
            .block_verifier
            .verify_consensus(&sealed_header)
        {
            return Ok(());
        }
        classify_fcr_candidate(
            shared,
            published,
            candidate.da_height(),
            candidate.event_inbox_root(),
            || {
                let previous_height = candidate
                    .height()
                    .pred()
                    .ok_or_else(|| anyhow::anyhow!("Genesis has no event prefix"))?;
                let Some(previous) = self
                    .database
                    .storage_as_ref::<FuelBlocks>()
                    .get(&previous_height)?
                else {
                    return Ok(None);
                };
                let mut root = MerkleRootCalculator::new();
                for height in previous.header().da_height().0.saturating_add(1)
                    ..=candidate.da_height().0
                {
                    for event in adapter.relayer_database.get_events(&height.into())? {
                        root.push(event.hash().as_ref());
                    }
                }
                Ok(Some(fuel_core_types::fuel_types::Bytes32::from(
                    root.root(),
                )))
            },
        )
    }
}

#[cfg(feature = "relayer")]
fn classify_fcr_candidate(
    shared: &fuel_core_relayer::SharedState,
    published: fuel_core_types::blockchain::primitives::DaBlockHeight,
    candidate: fuel_core_types::blockchain::primitives::DaBlockHeight,
    expected: Bytes32,
    read_root: impl FnOnce() -> anyhow::Result<Option<Bytes32>>,
) -> anyhow::Result<()> {
    anyhow::ensure!(candidate <= published, "Local DA catch-up pending");
    if let Some(actual) = read_root()? {
        let agrees = actual == expected;
        shared.set_import_held(!agrees);
        anyhow::ensure!(agrees, "Covered canonical event-root divergence");
    }
    Ok(())
}

#[cfg(all(test, feature = "relayer"))]
mod fcr_classification_tests {
    use super::*;
    #[tokio::test]
    async fn fcr_lag_covered_mismatch_and_repair_use_the_sampled_frontier() {
        let service = fuel_core_relayer::new_service(
            Database::<crate::database::database_description::relayer::Relayer>::default(
            ),
            fuel_core_relayer::Config {
                relayer: Some(vec!["http://127.0.0.1:1".parse().unwrap()]),
                ..Default::default()
            },
        )
        .unwrap();
        let shared = service.shared.clone();
        let expected = Bytes32::from([1; 32]);
        let different = Bytes32::from([2; 32]);
        let published = 5u64.into();
        let mut advanced = 5u64;
        assert!(
            classify_fcr_candidate(&shared, published, 8u64.into(), expected, || {
                advanced = 10;
                Ok(Some(different))
            })
            .is_err()
        );
        assert_eq!(
            advanced, 5,
            "lag rejects before reading roots, even if a concurrent read could advance the relayer"
        );
        assert!(shared.ensure_production_allowed().is_ok());
        advanced = 10;
        assert!(
            classify_fcr_candidate(&shared, published, 8u64.into(), expected, || Ok(
                Some(different)
            ))
            .is_err()
        );
        assert!(
            shared.ensure_production_allowed().is_ok(),
            "a later frontier cannot relabel the original lag as divergence"
        );
        assert!(
            classify_fcr_candidate(
                &shared,
                advanced.into(),
                8u64.into(),
                expected,
                || Ok(Some(different))
            )
            .is_err()
        );
        assert!(shared.ensure_production_allowed().is_err());
        classify_fcr_candidate(&shared, advanced.into(), 8u64.into(), expected, || {
            Ok(Some(expected))
        })
        .unwrap();
        assert!(shared.ensure_production_allowed().is_ok());
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
