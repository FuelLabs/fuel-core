use super::import_result_provider::{
    self,
};
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
use fuel_core_storage::transactional::{
    AtomicView,
    HistoricalView,
};
use fuel_core_types::{
    blockchain::block::Block,
    fuel_types::ChainId,
    services::block_importer::SharedImportResult,
};

/// Provides the necessary functionality for accessing latest and historical block data.
pub struct CompressionBlockDBAdapter {
    block_importer: BlockImporterAdapter,
    db: Database<OnChain>,
}

impl CompressionBlockDBAdapter {
    pub fn new(block_importer: BlockImporterAdapter, db: Database<OnChain>) -> Self {
        Self { block_importer, db }
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

impl block_source::BlockSource for CompressionBlockDBAdapter {
    fn subscribe(&self) -> fuel_core_services::stream::BoxStream<SharedImportResult> {
        self.block_importer.events_shared_result()
    }

    fn get_block(&self, height: BlockAt) -> anyhow::Result<Block> {
        let latest_view = self.db.latest_view()?;
        let height = match height {
            BlockAt::Genesis => latest_view.genesis_height()?.ok_or_else(|| {
                anyhow::anyhow!("Genesis block not found in the database")
            })?,
            BlockAt::Specific(h) => h.into(),
        };
        let block = latest_view
            .get_sealed_block_by_height(&height)?
            .map(|sealed_block| sealed_block.entity)
            .ok_or(anyhow::anyhow!("Block not found at height: {:?}", height))?;
        Ok(block)
    }
}

impl configuration::CompressionConfigProvider
    for crate::service::config::DaCompressionConfig
{
    fn config(&self, chain_id: ChainId) -> config::CompressionConfig {
        config::CompressionConfig::new(
            self.retention_duration,
            self.starting_height,
            self.metrics,
            chain_id,
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
