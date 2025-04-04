use std::time::Instant;

use crate::{
    config::CompressionConfig,
    metrics::CompressionMetricsManager,
    ports::{
        block_source::{
            BlockSource,
            BlockWithMetadata,
            BlockWithMetadataExt,
        },
        compression_storage::{
            CompressionStorage,
            WriteCompressedBlock,
        },
        configuration::CompressionConfigProvider,
    },
    sync_state::{
        new_sync_state_channel,
        SyncStateNotifier,
        SyncStateObserver,
    },
    temporal_registry::{
        CompressionContext,
        CompressionStorageWrapper,
    },
};
use fuel_core_compression::compress::compress;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_storage::transactional::WriteTransaction;
use futures::FutureExt;

/// The compression service.
/// Responsible for subscribing to the l2 block stream,
/// perform compression of the block, and store the compressed block.
/// We don't need to share any data between this task and anything else yet(?)
/// perhaps we want to expose a way to get the registry root
pub struct CompressionService<S> {
    /// The block stream.
    block_stream: crate::ports::block_source::BlockStream,
    /// The compression storage.
    storage: S,
    /// The compression config.
    config: CompressionConfig,
    /// The sync notifier
    sync_notifier: SyncStateNotifier,
    /// metrics manager
    metrics_manager: Option<CompressionMetricsManager>,
}

impl<S> CompressionService<S>
where
    S: CompressionStorage,
{
    fn new(
        block_stream: crate::ports::block_source::BlockStream,
        storage: S,
        config: CompressionConfig,
    ) -> Self {
        let (sync_notifier, _) = new_sync_state_channel();

        let metrics_manager = if config.metrics() {
            Some(CompressionMetricsManager::new())
        } else {
            None
        };

        Self {
            block_stream,
            storage,
            config,
            sync_notifier,
            metrics_manager,
        }
    }
}

impl<S> CompressionService<S>
where
    S: CompressionStorage,
{
    fn compress_block(
        &mut self,
        block_with_metadata: &BlockWithMetadata,
    ) -> crate::Result<usize> {
        let mut storage_tx = self.storage.write_transaction();

        // compress the block
        let compression_context = CompressionContext {
            compression_storage: CompressionStorageWrapper {
                storage_tx: &mut storage_tx,
            },
            block_events: block_with_metadata.events(),
        };
        let compressed_block = compress(
            self.config.into(),
            compression_context,
            block_with_metadata.block(),
        )
        .now_or_never()
        .expect("The current implementation should resolve all futures instantly")
        .map_err(crate::errors::CompressionError::FailedToCompressBlock)?;

        let size_of_compressed_block = storage_tx
            .write_compressed_block(block_with_metadata.height(), &compressed_block)?;

        storage_tx
            .commit()
            .map_err(crate::errors::CompressionError::FailedToCommitTransaction)?;

        Ok(size_of_compressed_block)
    }

    fn handle_new_block(
        &mut self,
        block_with_metadata: &BlockWithMetadata,
    ) -> crate::Result<()> {
        // set the status to not synced
        self.sync_notifier
            .send(crate::sync_state::SyncState::NotSynced)
            .ok();

        if let Some(metrics_manager) = self.metrics_manager {
            let (compressed_block_size, compression_duration) = {
                let start = Instant::now();
                let compressed_block_size = self.compress_block(block_with_metadata)?;
                (compressed_block_size, start.elapsed().as_millis())
            };

            metrics_manager.record_compression_duration_ms(compression_duration);
            metrics_manager.record_compressed_block_size(compressed_block_size);
            metrics_manager
                .record_compression_block_height(*block_with_metadata.height());
        } else {
            self.compress_block(block_with_metadata)?;
        }

        // set the status to synced
        self.sync_notifier
            .send(crate::sync_state::SyncState::Synced(
                *block_with_metadata.height(),
            ))
            .ok();
        Ok(())
    }
}

/// Shared data for the compression service.
#[derive(Debug, Clone)]
pub struct SharedData {
    /// Allows to observe the sync state.
    sync_observer: SyncStateObserver,
}

impl SharedData {
    /// Waits until the compression service has synced
    /// with current l2 block height
    pub async fn await_synced(&self) -> crate::Result<()> {
        let mut observer = self.sync_observer.clone();
        loop {
            if observer.borrow_and_update().is_synced() {
                break;
            }

            observer
                .changed()
                .await
                .map_err(|_| crate::errors::CompressionError::FailedToGetSyncStatus)?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl<S> RunnableService for CompressionService<S>
where
    S: CompressionStorage + Send + Sync,
{
    const NAME: &'static str = "CompressionService";
    type Task = Self;
    type SharedData = SharedData;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        SharedData {
            sync_observer: self.sync_notifier.subscribe(),
        }
    }

    async fn into_task(
        self,
        _state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}

impl<S> RunnableTask for CompressionService<S>
where
    S: CompressionStorage + Send + Sync,
{
    async fn run(
        &mut self,
        watcher: &mut StateWatcher,
    ) -> fuel_core_services::TaskNextAction {
        use futures::StreamExt;

        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                fuel_core_services::TaskNextAction::Stop
            }

            block_res = self.block_stream.next() => {
                match block_res {
                    None => {
                        tracing::warn!("No blocks available");
                        fuel_core_services::TaskNextAction::Stop
                    }
                    Some(block_with_metadata) => {
                        tracing::debug!("Got new block: {:?}", &block_with_metadata.height());
                        if let Err(e) = self.handle_new_block(&block_with_metadata) {
                            tracing::error!("Error handling new block: {:?}", e);
                            return fuel_core_services::TaskNextAction::ErrorContinue(anyhow::anyhow!(e));
                        };
                        fuel_core_services::TaskNextAction::Continue
                    }
                }
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Create a new compression service.
pub fn new_service<B, S, C>(
    block_source: B,
    storage: S,
    config_provider: C,
) -> crate::Result<ServiceRunner<CompressionService<S>>>
where
    B: BlockSource + Send + Sync,
    S: CompressionStorage + Send + Sync,
    C: CompressionConfigProvider + Send + Sync,
{
    let config = config_provider.config();
    let block_stream = block_source.subscribe();
    Ok(ServiceRunner::new(CompressionService::new(
        block_stream,
        storage,
        config,
    )))
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::block_source::{
            BlockWithMetadata,
            BlockWithMetadataExt,
        },
        storage,
    };
    use fuel_core_services::{
        stream::{
            BoxStream,
            IntoBoxStream,
        },
        Service,
    };
    use fuel_core_storage::{
        merkle::column::MerkleizedColumn,
        structured_storage::test::InMemoryStorage,
        transactional::{
            IntoTransaction,
            StorageTransaction,
        },
        StorageAsRef,
    };

    struct EmptyBlockSource;

    impl BlockSource for EmptyBlockSource {
        fn subscribe(&self) -> BoxStream<BlockWithMetadata> {
            tokio_stream::pending().into_boxed()
        }
    }

    type MockStorage = StorageTransaction<
        InMemoryStorage<MerkleizedColumn<crate::storage::column::CompressionColumn>>,
    >;

    fn test_storage() -> MockStorage {
        InMemoryStorage::default().into_transaction()
    }

    struct MockConfigProvider(crate::config::CompressionConfig);

    impl Default for MockConfigProvider {
        fn default() -> Self {
            Self(crate::config::CompressionConfig::new(
                std::time::Duration::from_secs(10),
                false,
            ))
        }
    }

    impl CompressionConfigProvider for MockConfigProvider {
        fn config(&self) -> crate::config::CompressionConfig {
            self.0
        }
    }

    #[tokio::test]
    async fn compression_service__can_be_started_and_stopped() {
        // given
        let block_source = EmptyBlockSource;
        let storage = test_storage();
        let config_provider = MockConfigProvider::default();

        let service = new_service(block_source, storage, config_provider).unwrap();

        // when
        service.start_and_await().await.unwrap();

        // then
        // no assertions, just checking if it runs without panicking
        service.stop_and_await().await.unwrap();
    }

    struct MockBlockSource(Vec<BlockWithMetadata>);

    impl MockBlockSource {
        fn new(blocks: Vec<BlockWithMetadata>) -> Self {
            Self(blocks)
        }
    }

    impl BlockSource for MockBlockSource {
        fn subscribe(&self) -> BoxStream<BlockWithMetadata> {
            tokio_stream::iter(self.0.clone()).into_boxed()
        }
    }

    #[tokio::test]
    async fn compression_service__run__does_not_compress_blocks_if_no_blocks_provided() {
        // given
        // we provide a block source that will return None upon calling .next()
        let block_source = MockBlockSource::new(vec![]);
        let storage = test_storage();
        let config_provider = MockConfigProvider::default();

        let mut service = CompressionService::new(
            block_source.subscribe(),
            storage,
            config_provider.config(),
        );

        // when
        let _ = service.run(&mut StateWatcher::started()).await;

        // then
        let maybe_block = service
            .storage
            .storage_as_ref::<storage::CompressedBlocks>()
            .get(&0.into())
            .unwrap();
        assert!(maybe_block.is_none());
    }

    #[tokio::test]
    async fn compression_service__run__compresses_blocks() {
        // given
        // we provide a block source that will return a block upon calling .next()
        let block_with_metadata = BlockWithMetadata::default();
        let block_source = MockBlockSource::new(vec![block_with_metadata]);
        let storage = test_storage();
        let config_provider = MockConfigProvider::default();

        let mut service = CompressionService::new(
            block_source.subscribe(),
            storage,
            config_provider.config(),
        );
        let sync_observer = service.shared_data();

        // when
        let _ = service.run(&mut StateWatcher::started()).await;

        // then
        sync_observer.await_synced().await.unwrap();
        let maybe_block = service
            .storage
            .storage_as_ref::<storage::CompressedBlocks>()
            .get(&0.into())
            .unwrap();
        assert!(maybe_block.is_some());
    }
}
