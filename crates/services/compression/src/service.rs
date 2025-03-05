use crate::ports::{
    block_source::BlockSource,
    compression_storage::CompressionStorage,
    configuration::CompressionConfigProvider,
};
use fuel_core_services::{
    EmptyShared,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};

/// The compression service.
/// Responsible for subscribing to the l2 block stream,
/// perform compression of the block, and store the compressed block.
/// We don't need to share any data between this task and anything else yet(?)
/// perhaps we want to expose a way to get the registry root
pub struct CompressionService<B, S, C> {
    /// The block source.
    block_source: B,
    /// The compression storage.
    storage: S,
    /// The compression config.
    config: C,
}

impl<B, S, C> CompressionService<B, S, C>
where
    B: BlockSource,
    S: CompressionStorage,
    C: CompressionConfigProvider,
{
    fn new(block_source: B, storage: S, config: C) -> Self {
        Self {
            block_source,
            storage,
            config,
        }
    }

    fn handle_new_block(
        &self,
        _block: crate::ports::block_source::Block,
    ) -> crate::Result<()> {
        // compress the block
        // store the compressed block
        // get registry root (?) and push to shared state
        Ok(())
    }
}

#[async_trait::async_trait]
impl<B, S, C> RunnableService for CompressionService<B, S, C>
where
    B: BlockSource + Send + Sync,
    S: CompressionStorage + Send + Sync,
    C: CompressionConfigProvider + Send + Sync,
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

impl<B, S, C> RunnableTask for CompressionService<B, S, C>
where
    B: BlockSource + Send + Sync,
    S: CompressionStorage + Send + Sync,
    C: CompressionConfigProvider + Send + Sync,
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

            block_res = self.block_source.next() => {
                match block_res {
                    Err(e) => {
                        tracing::error!("Error getting next block: {:?}", e);
                        fuel_core_services::TaskNextAction::ErrorContinue(anyhow::anyhow!(e))
                    }
                    Ok(None) => {
                        tracing::warn!("No blocks available");
                        fuel_core_services::TaskNextAction::Stop
                    }
                    Ok(Some(block)) => {
                        tracing::debug!("Got new block: {:?}", block);
                        self.handle_new_block(block).unwrap();
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
    config: C,
) -> crate::Result<ServiceRunner<CompressionService<B, S, C>>>
where
    B: BlockSource + Send + Sync,
    S: CompressionStorage + Send + Sync,
    C: CompressionConfigProvider + Send + Sync,
{
    Ok(ServiceRunner::new(CompressionService::new(
        block_source,
        storage,
        config,
    )))
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{
        block_source::Block,
        compression_storage::CompressedBlock,
    };
    use fuel_core_services::Service;

    struct EmptyBlockSource;

    impl BlockSource for EmptyBlockSource {
        async fn next(&self) -> crate::Result<Option<Block>> {
            Ok(None)
        }
    }

    struct MockStorage;

    impl CompressionStorage for MockStorage {
        fn write_compressed_block(
            &mut self,
            payload: &CompressedBlock,
        ) -> crate::Result<()> {
            Ok(())
        }
    }

    struct MockConfig;

    impl CompressionConfigProvider for MockConfig {
        fn config(&self) -> crate::config::CompressionConfig {
            crate::config::CompressionConfig {}
        }
    }

    #[tokio::test]
    async fn compression_service__can_be_started_and_stopped() {
        // given
        let block_source = EmptyBlockSource;
        let storage = MockStorage;
        let config = MockConfig;

        let mut service = new_service(block_source, storage, config).unwrap();

        // when
        service.start_and_await().await.unwrap();

        // then
        // no assertions, just checking if it runs without panicking
        service.stop_and_await().await.unwrap();
    }
}
