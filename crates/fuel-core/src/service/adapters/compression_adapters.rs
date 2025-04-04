use crate::{
    database::{
        database_description::compression::CompressionDatabase,
        Database,
    },
    service::adapters::BlockImporterAdapter,
};
use fuel_core_compression_service::{
    config,
    ports::{
        block_source,
        configuration,
    },
};
use fuel_core_types::services::block_importer::SharedImportResult;

impl block_source::BlockSource for BlockImporterAdapter {
    fn subscribe(&self) -> fuel_core_services::stream::BoxStream<SharedImportResult> {
        self.events_shared_result()
    }
}

impl configuration::CompressionConfigProvider
    for crate::service::config::DaCompressionConfig
{
    fn config(&self) -> config::CompressionConfig {
        config::CompressionConfig::new(self.retention_duration, self.metrics)
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
