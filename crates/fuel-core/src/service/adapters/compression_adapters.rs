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
use fuel_core_services::stream::IntoBoxStream;

impl block_source::BlockSource for BlockImporterAdapter {
    fn subscribe(
        &self,
    ) -> fuel_core_services::stream::BoxStream<block_source::BlockWithMetadata> {
        use futures::StreamExt;
        self.events_shared_result()
            .map(|result| {
                let sealed_block = result.sealed_block.clone();
                let events = result.events.clone();
                block_source::BlockWithMetadata::new(sealed_block.entity, events)
            })
            .into_boxed()
    }
}

impl configuration::CompressionConfigProvider
    for crate::service::config::DaCompressionConfig
{
    fn config(&self) -> config::CompressionConfig {
        config::CompressionConfig::new(self.retention_duration)
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
