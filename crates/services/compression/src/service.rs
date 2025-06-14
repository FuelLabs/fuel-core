use crate::{
    config::CompressionConfig,
    metrics::CompressionMetricsManager,
    ports::{
        block_source::{
            BlockAt,
            BlockSource,
            BlockWithMetadata,
            BlockWithMetadataExt,
        },
        canonical_height::CanonicalHeight,
        compression_storage::{
            CompressionStorage,
            LatestHeight,
            WriteCompressedBlock,
        },
        configuration::CompressionConfigProvider,
    },
    sync_state::{
        SyncStateNotifier,
        SyncStateObserver,
        new_sync_state_channel,
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
use futures::{
    FutureExt,
    StreamExt,
};
use std::time::Instant;

/// Uninitialized compression service.
pub struct UninitializedCompressionService<B, S, CH> {
    /// The block source.
    block_source: B,
    /// The canonical height getter.
    canonical_height: CH,
    /// The compression storage.
    storage: S,
    /// The compression config.
    config: CompressionConfig,
    /// The sync notifier
    sync_notifier: SyncStateNotifier,
    /// metrics manager
    metrics_manager: Option<CompressionMetricsManager>,
}

impl<B, S, CH> UninitializedCompressionService<B, S, CH> {
    fn new(
        block_source: B,
        storage: S,
        config: CompressionConfig,
        canonical_height: CH,
    ) -> Self {
        let (sync_notifier, _) = new_sync_state_channel();

        let metrics_manager = if config.metrics() {
            Some(CompressionMetricsManager::new())
        } else {
            None
        };

        Self {
            block_source,
            canonical_height,
            storage,
            config,
            sync_notifier,
            metrics_manager,
        }
    }
}

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
    /// metrics manager,
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
        sync_notifier: SyncStateNotifier,
        metrics_manager: Option<CompressionMetricsManager>,
    ) -> Self {
        Self {
            block_stream,
            storage,
            config,
            sync_notifier,
            metrics_manager,
        }
    }
}

/// reused by uninit and init task
fn compress_block<S>(
    storage: &mut S,
    block_with_metadata: &BlockWithMetadata,
    config: &CompressionConfig,
) -> crate::Result<usize>
where
    S: CompressionStorage,
{
    let mut storage_tx = storage.write_transaction();

    // compress the block
    let compression_context = CompressionContext {
        compression_storage: CompressionStorageWrapper {
            storage_tx: &mut storage_tx,
        },
        block_events: block_with_metadata.events(),
    };
    let compressed_block = compress(
        &config.into(),
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

fn handle_new_block<S>(
    storage: &mut S,
    block_with_metadata: &BlockWithMetadata,
    config: &CompressionConfig,
    sync_notifier: &SyncStateNotifier,
    metrics_manager: &Option<CompressionMetricsManager>,
) -> crate::Result<()>
where
    S: CompressionStorage,
{
    // set the status to not synced
    sync_notifier
        .send(crate::sync_state::SyncState::NotSynced)
        .ok();
    // compress the block
    if let Some(metrics_manager) = metrics_manager {
        let (compressed_block_size, compression_duration) = {
            let start = Instant::now();
            let compressed_block_size =
                compress_block(storage, block_with_metadata, config)?;
            (compressed_block_size, start.elapsed().as_secs_f64())
        };

        metrics_manager.record_compression_duration_ms(compression_duration);
        metrics_manager.record_compressed_block_size(compressed_block_size);
        metrics_manager.record_compression_block_height(*block_with_metadata.height());
    } else {
        compress_block(storage, block_with_metadata, config)?;
    }
    // set the status to synced
    sync_notifier
        .send(crate::sync_state::SyncState::Synced(
            *block_with_metadata.height(),
        ))
        .ok();

    Ok(())
}

impl<S> CompressionService<S>
where
    S: CompressionStorage,
{
    fn handle_new_block(
        &mut self,
        block_with_metadata: &BlockWithMetadata,
    ) -> crate::Result<()> {
        handle_new_block(
            &mut self.storage,
            block_with_metadata,
            &self.config,
            &self.sync_notifier,
            &self.metrics_manager,
        )
    }
}

#[derive(Debug)]
enum SyncHeight {
    StorageHeight(u32),
    ConfiguredHeight(u32),
    Genesis,
}

impl SyncHeight {
    #[inline]
    const fn value(&self) -> u32 {
        match self {
            SyncHeight::StorageHeight(height) => *height,
            SyncHeight::ConfiguredHeight(height) => *height,
            SyncHeight::Genesis => 0,
        }
    }

    #[inline]
    const fn is_from_storage(&self) -> bool {
        matches!(self, SyncHeight::StorageHeight(_))
    }
}

impl<B, S, CH> UninitializedCompressionService<B, S, CH>
where
    B: BlockSource,
    S: CompressionStorage + LatestHeight,
    CH: CanonicalHeight,
{
    async fn sync_previously_produced_blocks(
        &mut self,
        state_watcher: &StateWatcher,
    ) -> crate::Result<()> {
        loop {
            // allows early exit if the service is stopping
            let state = state_watcher.borrow();
            if !state.starting() {
                break;
            }

            let canonical_height = match self.canonical_height.get() {
                Some(height) => height,
                None => {
                    // fuel-core started for first time,
                    // no need to backfill blocks
                    break;
                }
            };

            let maybe_height =
                match (self.storage.latest_height(), self.config.starting_height()) {
                    (Some(height), _) => SyncHeight::StorageHeight(height),
                    (None, Some(height)) => SyncHeight::ConfiguredHeight(height),
                    (None, None) => SyncHeight::Genesis,
                };

            if canonical_height < maybe_height.value() {
                tracing::error!(
                    "Canonical height is less than fetched height: Canonical height: {:?}, Fetched height: {:?}",
                    &canonical_height,
                    &maybe_height
                );
                return Err(crate::errors::CompressionError::FailedToGetSyncStatus);
            }

            if canonical_height == maybe_height.value() && maybe_height.is_from_storage()
            {
                tracing::info!("Compression database is up to date");
                break;
            }

            let height_to_sync = match maybe_height {
                SyncHeight::Genesis => BlockAt::Genesis,
                SyncHeight::StorageHeight(height) => {
                    BlockAt::Specific(height.saturating_add(1))
                }
                SyncHeight::ConfiguredHeight(height) => BlockAt::Specific(height),
            };

            let block_with_metadata =
                self.block_source.get_block(height_to_sync).map_err(|err| {
                    crate::errors::CompressionError::FailedToGetBlock(format!(
                        "during synchronization of canonical chain at height: {:?}: {}",
                        height_to_sync, err
                    ))
                })?;

            handle_new_block(
                &mut self.storage,
                &block_with_metadata,
                &self.config,
                &self.sync_notifier,
                &self.metrics_manager,
            )?;
        }

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
    /// with the given block height
    pub async fn await_synced_until(&self, block_height: &u32) -> crate::Result<()> {
        let mut observer = self.sync_observer.clone();
        loop {
            if observer.borrow_and_update().is_synced_until(block_height) {
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
impl<B, S, CH> RunnableService for UninitializedCompressionService<B, S, CH>
where
    B: BlockSource,
    S: CompressionStorage + LatestHeight,
    CH: CanonicalHeight,
{
    const NAME: &'static str = "CompressionService";
    type Task = CompressionService<S>;
    type SharedData = SharedData;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        SharedData {
            sync_observer: self.sync_notifier.subscribe(),
        }
    }

    async fn into_task(
        mut self,
        state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        self.sync_previously_produced_blocks(state_watcher).await?;

        let compression_service = CompressionService::new(
            self.block_source.subscribe(),
            self.storage,
            self.config,
            self.sync_notifier,
            self.metrics_manager,
        );

        Ok(compression_service)
    }
}

impl<S> RunnableTask for CompressionService<S>
where
    S: CompressionStorage,
{
    async fn run(
        &mut self,
        watcher: &mut StateWatcher,
    ) -> fuel_core_services::TaskNextAction {
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

    async fn shutdown(mut self) -> anyhow::Result<()> {
        // gracefully handle all the remaining blocks in the stream and then stop
        if let Some(Some(block_with_metadata)) = self.block_stream.next().now_or_never() {
            if let Err(e) = self.handle_new_block(&block_with_metadata) {
                return Err(anyhow::anyhow!(e).context(format!(
                    "Couldn't compress block: {}. Shutting down. \
                            Node will be in indeterminate state upon restart. \
                            Suggested to delete compression database.",
                    block_with_metadata.height()
                )));
            }
        }
        Ok(())
    }
}

/// Create a new compression service.
pub fn new_service<B, S, C, CH>(
    block_source: B,
    storage: S,
    config_provider: C,
    canonical_height: CH,
) -> crate::Result<ServiceRunner<UninitializedCompressionService<B, S, CH>>>
where
    B: BlockSource,
    S: CompressionStorage + LatestHeight,
    C: CompressionConfigProvider,
    CH: CanonicalHeight,
{
    let config = config_provider.config();
    Ok(ServiceRunner::new(UninitializedCompressionService::new(
        block_source,
        storage,
        config,
        canonical_height,
    )))
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::{
        ports::block_source::{
            BlockWithMetadata,
            BlockWithMetadataExt,
        },
        storage,
        storage::CompressedBlocks,
    };
    use fuel_core_services::{
        Service,
        stream::{
            BoxStream,
            IntoBoxStream,
        },
    };
    use fuel_core_storage::{
        StorageAsRef,
        iter::{
            IterDirection,
            IteratorOverTable,
            changes_iterator::ChangesIterator,
        },
        merkle::column::MerkleizedColumn,
        structured_storage::test::InMemoryStorage,
        transactional::{
            IntoTransaction,
            StorageChanges,
            StorageTransaction,
        },
    };

    struct EmptyBlockSource;

    impl BlockSource for EmptyBlockSource {
        fn subscribe(&self) -> BoxStream<BlockWithMetadata> {
            tokio_stream::pending().into_boxed()
        }

        fn get_block(
            &self,
            _: crate::ports::block_source::BlockAt,
        ) -> anyhow::Result<BlockWithMetadata> {
            anyhow::bail!("Block not found")
        }
    }

    type MockStorage = StorageTransaction<
        InMemoryStorage<MerkleizedColumn<crate::storage::column::CompressionColumn>>,
    >;

    fn test_storage() -> MockStorage {
        InMemoryStorage::default().into_transaction()
    }

    impl LatestHeight for MockStorage {
        fn latest_height(&self) -> Option<u32> {
            let changes = StorageChanges::Changes(self.changes().clone());
            let view = ChangesIterator::new(&changes);
            let compressed_block_changes = view
                .iter_all_keys::<CompressedBlocks>(Some(IterDirection::Reverse))
                .next()
                .transpose()
                .unwrap();
            compressed_block_changes.map(Into::into)
        }
    }

    struct MockConfigProvider(crate::config::CompressionConfig);

    impl Default for MockConfigProvider {
        fn default() -> Self {
            Self(crate::config::CompressionConfig::new(
                std::time::Duration::from_secs(10),
                None,
                false,
            ))
        }
    }

    impl MockConfigProvider {
        fn new(config: crate::config::CompressionConfig) -> Self {
            Self(config)
        }
    }

    impl CompressionConfigProvider for MockConfigProvider {
        fn config(&self) -> crate::config::CompressionConfig {
            self.0
        }
    }

    #[derive(Default, Clone)]
    struct MockCanonicalHeightProvider(u32);

    impl MockCanonicalHeightProvider {
        fn new(height: u32) -> Self {
            Self(height)
        }
    }

    impl CanonicalHeight for MockCanonicalHeightProvider {
        fn get(&self) -> Option<u32> {
            match self.0 {
                0 => None,
                _ => Some(self.0),
            }
        }
    }

    #[tokio::test]
    async fn compression_service__can_be_started_and_stopped() {
        // given
        let block_source = EmptyBlockSource;
        let storage = test_storage();
        let config_provider = MockConfigProvider::default();
        let canonical_height_provider = MockCanonicalHeightProvider::default();

        let service = new_service(
            block_source,
            storage,
            config_provider,
            canonical_height_provider,
        )
        .unwrap();

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

        fn get_block(
            &self,
            height: crate::ports::block_source::BlockAt,
        ) -> anyhow::Result<BlockWithMetadata> {
            self.0
                .iter()
                .find(|block| height == *block.height())
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Block not found"))
        }
    }

    #[tokio::test]
    async fn compression_service__run__does_not_compress_blocks_if_no_blocks_provided() {
        // given
        // we provide a block source that will return None upon calling .next()
        let block_source = MockBlockSource::new(vec![]);
        let storage = test_storage();
        let config_provider = MockConfigProvider::default();
        let (sync_notifier, _) = new_sync_state_channel();

        let mut service = CompressionService::new(
            block_source.subscribe(),
            storage,
            config_provider.config(),
            sync_notifier,
            None,
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
        service.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn compression_service__run__compresses_blocks() {
        // given
        // we provide a block source that will return a block upon calling .next()
        let block_with_metadata = BlockWithMetadata::default();
        let block_source = MockBlockSource::new(vec![block_with_metadata]);
        let storage = test_storage();
        let config_provider = MockConfigProvider::default();
        let canonical_height_provider = MockCanonicalHeightProvider::default();

        let uninit_service = UninitializedCompressionService::new(
            block_source,
            storage,
            config_provider.config(),
            canonical_height_provider,
        );
        let sync_observer = uninit_service.shared_data();

        // when
        let mut service = uninit_service
            .into_task(&Default::default(), ())
            .await
            .unwrap();
        let _ = service.run(&mut StateWatcher::started()).await;

        // then
        let target_block_height = 0;
        sync_observer
            .await_synced_until(&target_block_height)
            .await
            .unwrap();
        let maybe_block = service
            .storage
            .storage_as_ref::<storage::CompressedBlocks>()
            .get(&target_block_height.into())
            .unwrap();
        assert!(maybe_block.is_some());
        service.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn compression_service__syncs_from_scratch_when_database_is_empty() {
        // given: we start the compression service, with a canonical height provider of height 5
        let block_count = 10;
        let mut blocks = Vec::with_capacity(block_count);
        for i in 0..u32::try_from(block_count).unwrap() {
            blocks.push(BlockWithMetadata::test_block_with_height(i));
        }
        let block_source = MockBlockSource::new(blocks);
        let storage = test_storage();
        let config_provider = MockConfigProvider::default();
        let canonical_height_provider = MockCanonicalHeightProvider::new(5);

        let uninit_service = UninitializedCompressionService::new(
            block_source,
            storage,
            config_provider.config(),
            canonical_height_provider.clone(),
        );

        // when: the syncing with canonical height provider occurs
        let service = uninit_service
            .into_task(&StateWatcher::starting(), ())
            .await
            .unwrap();

        // then: it has a compressed block for the canonical height
        let maybe_block = service
            .storage
            .storage_as_ref::<storage::CompressedBlocks>()
            .get(&canonical_height_provider.get().unwrap().into())
            .unwrap();
        assert!(maybe_block.is_some());
        service.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn compression_service__syncs_from_overridden_starting_height_when_provided() {
        // given: we start the compression service, with a canonical height provider of height 5,
        // and a config override of starting height 1
        let block_count = 10;
        let override_starting_height = 1;
        let mut blocks = Vec::with_capacity(block_count);
        for i in 0..u32::try_from(block_count).unwrap() {
            blocks.push(BlockWithMetadata::test_block_with_height(i));
        }
        let block_source = MockBlockSource::new(blocks);
        let storage = test_storage();
        let config_provider =
            MockConfigProvider::new(crate::config::CompressionConfig::new(
                std::time::Duration::from_secs(10),
                Some(NonZeroU32::new(override_starting_height).unwrap()),
                false,
            ));
        let canonical_height_provider = MockCanonicalHeightProvider::new(5);

        let uninit_service = UninitializedCompressionService::new(
            block_source,
            storage,
            config_provider.config(),
            canonical_height_provider.clone(),
        );

        // when: the syncing with canonical height provider occurs
        let service = uninit_service
            .into_task(&StateWatcher::starting(), ())
            .await
            .unwrap();

        // then: it has a block for the overridden starting height
        let maybe_block = service
            .storage
            .storage_as_ref::<storage::CompressedBlocks>()
            .get(&override_starting_height.into())
            .unwrap();
        assert!(maybe_block.is_some());
        service.shutdown().await.unwrap();
    }

    #[cfg(feature = "fault-proving")]
    #[tokio::test]
    async fn compression_service__writes_to_registrations_table() {
        // given: we create the compression service
        let block_with_metadata = BlockWithMetadata::default();
        let block_source = MockBlockSource::new(vec![block_with_metadata]);
        let storage = test_storage();
        let config_provider = MockConfigProvider::default();
        let canonical_height_provider = MockCanonicalHeightProvider::default();

        let uninit_service = UninitializedCompressionService::new(
            block_source,
            storage,
            config_provider.config(),
            canonical_height_provider,
        );
        let sync_observer = uninit_service.shared_data();

        // when: the compression service is started
        let mut service = uninit_service
            .into_task(&Default::default(), ())
            .await
            .unwrap();
        let _ = service.run(&mut StateWatcher::started()).await;

        // then: we ensure we can get the registrations per block + root of that table
        let target_block_height = 0;
        sync_observer
            .await_synced_until(&target_block_height)
            .await
            .unwrap();
        let maybe_registrations = service
            .storage
            .storage_as_ref::<storage::registrations::Registrations>()
            .get(&target_block_height.into())
            .unwrap();
        assert!(maybe_registrations.is_some());
    }
}
