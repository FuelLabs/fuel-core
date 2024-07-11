use crate::{
    database::Database,
    service::adapters::ExecutorAdapter,
};
use fuel_core_importer::ports::Validator;
use fuel_core_storage::{
    not_found,
    transactional::AtomicView,
};
use fuel_core_types::{
    fuel_types::BlockHeight,
    services::{
        block_importer::{
            ImportResult,
            SharedImportResult,
        },
        executor::ValidationResult,
    },
};
use std::sync::Arc;

#[derive(Clone)]
pub struct ImportResultProvider {
    on_chain_database: Database,
    executor_adapter: ExecutorAdapter,
}

impl ImportResultProvider {
    pub fn new(on_chain_database: Database, executor_adapter: ExecutorAdapter) -> Self {
        Self {
            on_chain_database,
            executor_adapter,
        }
    }
}

impl ImportResultProvider {
    pub fn result_at_height(
        &self,
        height: Option<BlockHeight>,
    ) -> anyhow::Result<SharedImportResult> {
        if let Some(height) = height {
            let sealed_block = self
                .on_chain_database
                .latest_view()?
                .get_sealed_block_by_height(&height)?
                .ok_or(not_found!("SealedBlock"))?;

            let ValidationResult { tx_status, events } = self
                .executor_adapter
                .validate(&sealed_block.entity)?
                .into_result();
            let result = ImportResult::new_from_local(sealed_block, tx_status, events);
            Ok(Arc::new(result))
        } else {
            let genesis_height = self
                .on_chain_database
                .latest_view()?
                .genesis_height()?
                .ok_or(not_found!("Genesis height"))?;
            let sealed_block = self
                .on_chain_database
                .latest_view()?
                .get_sealed_block_by_height(&genesis_height)?
                .ok_or(not_found!("SealedBlock"))?;

            Ok(Arc::new(ImportResult::new_from_local(
                sealed_block,
                vec![],
                vec![],
            )))
        }
    }
}
