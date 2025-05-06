use crate::{
    database::{
        Database,
        database_description::{
            compression::CompressionDatabase,
            on_chain::OnChain,
        },
    },
    service::adapters::BlockImporterAdapter,
};
use fuel_core_compression_service::{
    config,
    ports::{
        block_source::{
            self,
            BlockAt,
        },
        canonical_height,
        compression_storage,
        configuration,
    },
};
use fuel_core_storage::transactional::HistoricalView;
use fuel_core_types::services::block_importer::SharedImportResult;

use super::import_result_provider::{
    self,
    ImportResultProvider,
};

/// Provides the necessary functionality for accessing latest and historical block data.
pub struct CompressionBlockImporterAdapter {
    block_importer: BlockImporterAdapter,
    import_result_provider_adapter: ImportResultProvider,
}

impl CompressionBlockImporterAdapter {
    pub fn new(
        block_importer: BlockImporterAdapter,
        import_result_provider_adapter: ImportResultProvider,
    ) -> Self {
        Self {
            block_importer,
            import_result_provider_adapter,
        }
    }
}

impl From<BlockAt> for import_result_provider::BlockAt {
    fn from(value: BlockAt) -> Self {
        match value {
            BlockAt::Genesis => Self::Genesis,
            BlockAt::Specific(h) => Self::Specific(h.into()),
        }
    }
}

impl block_source::BlockSource for CompressionBlockImporterAdapter {
    fn subscribe(&self) -> fuel_core_services::stream::BoxStream<SharedImportResult> {
        self.block_importer.events_shared_result()
    }

    fn get_block(&self, height: BlockAt) -> anyhow::Result<SharedImportResult> {
        self.import_result_provider_adapter
            .result_at_height(height.into())
    }
}

impl configuration::CompressionConfigProvider
    for crate::service::config::DaCompressionConfig
{
    fn config(&self) -> config::CompressionConfig {
        config::CompressionConfig::new(
            self.retention_duration,
            self.starting_height,
            self.metrics,
        )
    }
}

impl compression_storage::LatestHeight for Database<CompressionDatabase> {
    fn latest_height(&self) -> Option<u32> {
        HistoricalView::latest_height(self).map(Into::into)
    }
}

impl canonical_height::CanonicalHeight for Database<OnChain> {
    fn get(&self) -> Option<u32> {
        HistoricalView::latest_height(self).map(Into::into)
    }
}

pub struct CompressionServiceAdapter {
    db: Database<CompressionDatabase>,
}

impl CompressionServiceAdapter {
    pub fn new(db: Database<CompressionDatabase>) -> Self {
        Self { db }
    }

    pub fn storage(&self) -> &Database<CompressionDatabase> {
        &self.db
    }
}
