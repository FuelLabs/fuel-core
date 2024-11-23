use crate::service::adapters::BlockImporterAdapter;
use fuel_core_services::stream::BoxStream;
use fuel_core_shared_sequencer::ports::BlocksProvider;
use fuel_core_types::services::block_importer::SharedImportResult;

impl BlocksProvider for BlockImporterAdapter {
    fn subscribe(&self) -> BoxStream<SharedImportResult> {
        self.events_shared_result()
    }
}
