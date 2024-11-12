use crate::{
    common::{
        l2_block_source::L2BlockSource,
        updater_metadata::UpdaterMetadata,
        utils::BlockInfo,
    },
    ports::MetadataStorage,
    v0::algorithm::SharedV0Algorithm,
};
use anyhow::anyhow;
use async_trait::async_trait;
use fuel_core_services::{
    RunnableTask,
    StateWatcher,
    TaskRunResult,
};
use fuel_gas_price_algorithm::v0::{
    AlgorithmUpdaterV0,
    AlgorithmV0,
};
use futures::FutureExt;
use std::num::NonZeroU64;

/// The service that updates the gas price algorithm.
pub struct GasPriceServiceV0<L2, Metadata> {
    /// The algorithm that can be used in the next block
    shared_algo: SharedV0Algorithm,
    /// The L2 block source
    l2_block_source: L2,
    /// The metadata storage
    metadata_storage: Metadata,
    /// The algorithm updater
    algorithm_updater: AlgorithmUpdaterV0,
}

impl<L2, Metadata> GasPriceServiceV0<L2, Metadata>
where
    Metadata: MetadataStorage,
{
    pub fn new(
        l2_block_source: L2,
        metadata_storage: Metadata,
        shared_algo: SharedV0Algorithm,
        algorithm_updater: AlgorithmUpdaterV0,
    ) -> Self {
        Self {
            shared_algo,
            l2_block_source,
            metadata_storage,
            algorithm_updater,
        }
    }

    pub fn algorithm_updater(&self) -> &AlgorithmUpdaterV0 {
        &self.algorithm_updater
    }

    pub fn next_block_algorithm(&self) -> SharedV0Algorithm {
        self.shared_algo.clone()
    }

    async fn update(&mut self, new_algorithm: AlgorithmV0) {
        self.shared_algo.update(new_algorithm).await;
    }

    fn validate_block_gas_capacity(
        &self,
        block_gas_capacity: u64,
    ) -> anyhow::Result<NonZeroU64> {
        NonZeroU64::new(block_gas_capacity)
            .ok_or_else(|| anyhow!("Block gas capacity must be non-zero"))
    }

    async fn set_metadata(&mut self) -> anyhow::Result<()> {
        let metadata: UpdaterMetadata = self.algorithm_updater.clone().into();
        self.metadata_storage
            .set_metadata(&metadata)
            .map_err(|err| anyhow!(err))
    }

    async fn handle_normal_block(
        &mut self,
        height: u32,
        gas_used: u64,
        block_gas_capacity: u64,
    ) -> anyhow::Result<()> {
        let capacity = self.validate_block_gas_capacity(block_gas_capacity)?;

        self.algorithm_updater
            .update_l2_block_data(height, gas_used, capacity)?;

        self.set_metadata().await?;
        Ok(())
    }

    async fn apply_block_info_to_gas_algorithm(
        &mut self,
        l2_block: BlockInfo,
    ) -> anyhow::Result<()> {
        match l2_block {
            BlockInfo::GenesisBlock => {
                self.set_metadata().await?;
            }
            BlockInfo::Block {
                height,
                gas_used,
                block_gas_capacity,
            } => {
                self.handle_normal_block(height, gas_used, block_gas_capacity)
                    .await?;
            }
        }

        self.update(self.algorithm_updater.algorithm()).await;
        Ok(())
    }
}

#[async_trait]
impl<L2, Metadata> RunnableTask for GasPriceServiceV0<L2, Metadata>
where
    L2: L2BlockSource,
    Metadata: MetadataStorage,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskRunResult {
        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                tracing::debug!("Stopping gas price service");
                TaskRunResult::Stop
            }
            l2_block_res = self.l2_block_source.get_l2_block() => {
                tracing::info!("Received L2 block result: {:?}", l2_block_res);
                let block = match l2_block_res {
                    Ok(block) => block,
                    Err(err) => {
                        return anyhow!(err).into()
                    }
                };

                tracing::debug!("Updating gas price algorithm");
                match self.apply_block_info_to_gas_algorithm(block).await {
                    Ok(_) => {},
                    Err(err) => {
                        return err.into()
                    }
                }
                TaskRunResult::Continue
            }
        }
    }

    async fn shutdown(mut self) -> anyhow::Result<()> {
        while let Some(Ok(block)) = self.l2_block_source.get_l2_block().now_or_never() {
            tracing::debug!("Updating gas price algorithm");
            self.apply_block_info_to_gas_algorithm(block).await?;
        }
        Ok(())
    }
}

#[allow(clippy::arithmetic_side_effects)]
#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use crate::{
        common::{
            l2_block_source::L2BlockSource,
            updater_metadata::UpdaterMetadata,
            utils::{
                BlockInfo,
                Result as GasPriceResult,
            },
        },
        ports::MetadataStorage,
        v0::{
            metadata::V0AlgorithmConfig,
            service::GasPriceServiceV0,
            uninitialized_task::initialize_algorithm,
        },
    };
    use fuel_core_services::{
        RunnableTask,
        StateWatcher,
    };
    use fuel_core_types::fuel_types::BlockHeight;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    struct FakeL2BlockSource {
        l2_block: mpsc::Receiver<BlockInfo>,
    }

    #[async_trait::async_trait]
    impl L2BlockSource for FakeL2BlockSource {
        async fn get_l2_block(&mut self) -> GasPriceResult<BlockInfo> {
            let block = self.l2_block.recv().await.unwrap();
            Ok(block)
        }
    }

    struct FakeMetadata {
        inner: Arc<std::sync::Mutex<Option<UpdaterMetadata>>>,
    }

    impl FakeMetadata {
        fn empty() -> Self {
            Self {
                inner: Arc::new(std::sync::Mutex::new(None)),
            }
        }
    }

    impl MetadataStorage for FakeMetadata {
        fn get_metadata(
            &self,
            _: &BlockHeight,
        ) -> GasPriceResult<Option<UpdaterMetadata>> {
            let metadata = self.inner.lock().unwrap().clone();
            Ok(metadata)
        }

        fn set_metadata(&mut self, metadata: &UpdaterMetadata) -> GasPriceResult<()> {
            *self.inner.lock().unwrap() = Some(metadata.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn run__updates_gas_price() {
        // given
        let block_height = 1;
        let l2_block = BlockInfo::Block {
            height: block_height,
            gas_used: 60,
            block_gas_capacity: 100,
        };

        let (l2_block_sender, l2_block_receiver) = mpsc::channel(1);
        let l2_block_source = FakeL2BlockSource {
            l2_block: l2_block_receiver,
        };

        let metadata_storage = FakeMetadata::empty();
        let l2_block_height = 0;
        let config = V0AlgorithmConfig {
            starting_gas_price: 100,
            min_gas_price: 10,
            gas_price_change_percent: 10,
            gas_price_threshold_percent: 0,
        };
        let (algo_updater, shared_algo) =
            initialize_algorithm(&config, l2_block_height, &metadata_storage).unwrap();
        let mut service = GasPriceServiceV0::new(
            l2_block_source,
            metadata_storage,
            shared_algo,
            algo_updater,
        );
        let read_algo = service.next_block_algorithm();
        let mut watcher = StateWatcher::default();
        let initial_price = read_algo.next_gas_price();

        // when
        service.run(&mut watcher).await;
        l2_block_sender.send(l2_block).await.unwrap();
        service.shutdown().await.unwrap();

        // then
        let actual_price = read_algo.next_gas_price();
        assert_ne!(initial_price, actual_price);
    }
}
