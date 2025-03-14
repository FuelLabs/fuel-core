use crate::{
    config::CompressionConfig,
    ports::{
        block_source::BlockSource,
        compression_storage::{
            CompressionStorage,
            WriteCompressedBlock,
        },
        configuration::CompressionConfigProvider,
    },
    temporal_registry::{
        CompressionContext,
        CompressionStorageWrapper,
    },
};
use fuel_core_compression::compress::compress;
use fuel_core_services::{
    EmptyShared,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
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
}

use fuel_core_storage::transactional::WriteTransaction;

impl<S> CompressionService<S>
where
    S: CompressionStorage,
{
    fn new(
        block_stream: crate::ports::block_source::BlockStream,
        storage: S,
        config: CompressionConfig,
    ) -> Self {
        Self {
            block_stream,
            storage,
            config,
        }
    }
}

impl<S> CompressionService<S>
where
    S: CompressionStorage,
{
    fn compress_block(
        &mut self,
        block_with_metadata: &crate::ports::block_source::BlockWithMetadata,
    ) -> crate::Result<()> {
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

        storage_tx
            .write_compressed_block(&block_with_metadata.height(), &compressed_block)?;

        storage_tx
            .commit()
            .map_err(crate::errors::CompressionError::FailedToCommitTransaction)?;

        Ok(())
    }

    fn handle_new_block(
        &mut self,
        block_with_metadata: &crate::ports::block_source::BlockWithMetadata,
    ) -> crate::Result<()> {
        // compress the block
        self.compress_block(block_with_metadata)?;
        // get registry root (?) and push to shared state
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
    type SharedData = EmptyShared;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        EmptyShared
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
                        tracing::debug!("Got new block: {:?}", block_with_metadata.height());
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
        ports::block_source::BlockWithMetadata,
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

        // when
        let _ = service.run(&mut StateWatcher::started()).await;

        // then
        let maybe_block = service
            .storage
            .storage_as_ref::<storage::CompressedBlocks>()
            .get(&0.into())
            .unwrap();
        assert!(maybe_block.is_some());
    }
}
