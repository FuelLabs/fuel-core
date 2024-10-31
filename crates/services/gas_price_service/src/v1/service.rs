use crate::{
    common::{
        gas_price_algorithm::SharedGasPriceAlgo,
        l2_block_source::L2BlockSource,
        updater_metadata::UpdaterMetadata,
        utils::BlockInfo,
    },
    ports::MetadataStorage,
    v0::metadata::V0Metadata,
    v1::{
        algorithm::SharedV1Algorithm,
        da_source_service::{
            service::{
                DaBlockCostsSource,
                DaSourceService,
                SharedState as DaSharedState,
            },
            DaBlockCosts,
        },
        metadata::{
            v1_algorithm_from_metadata,
            V1AlgorithmConfig,
            V1Metadata,
        },
    },
};
use anyhow::anyhow;
use async_trait::async_trait;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
};
use fuel_gas_price_algorithm::{
    v0::AlgorithmUpdaterV0,
    v1::{
        AlgorithmUpdaterV1,
        AlgorithmV1,
    },
};
use futures::FutureExt;
use std::num::NonZeroU64;
use tokio::sync::broadcast::Receiver;

/// The service that updates the gas price algorithm.
pub struct GasPriceServiceV1<L2, Metadata, DA>
where
    DA: DaBlockCostsSource,
{
    /// The algorithm that can be used in the next block
    shared_algo: SharedV1Algorithm,
    /// The L2 block source
    l2_block_source: L2,
    /// The metadata storage
    metadata_storage: Metadata,
    /// The algorithm updater
    algorithm_updater: AlgorithmUpdaterV1,
    /// the da source adapter handle
    da_source_adapter_handle: DaSourceService<DA>,
    /// The da source channel
    da_source_channel: Receiver<DaBlockCosts>,
}

impl<L2, Metadata, DA> GasPriceServiceV1<L2, Metadata, DA>
where
    Metadata: MetadataStorage,
    DA: DaBlockCostsSource,
{
    pub fn new(
        l2_block_source: L2,
        metadata_storage: Metadata,
        shared_algo: SharedV1Algorithm,
        algorithm_updater: AlgorithmUpdaterV1,
        da_source_adapter_handle: DaSourceService<DA>,
    ) -> Self {
        let da_source_channel =
            da_source_adapter_handle.shared_data().clone().subscribe();
        Self {
            shared_algo,
            l2_block_source,
            metadata_storage,
            algorithm_updater,
            da_source_adapter_handle,
            da_source_channel,
        }
    }

    pub fn algorithm_updater(&self) -> &AlgorithmUpdaterV1 {
        &self.algorithm_updater
    }

    pub fn next_block_algorithm(&self) -> SharedV1Algorithm {
        self.shared_algo.clone()
    }

    async fn update(&mut self, new_algorithm: AlgorithmV1) {
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
            .update_l2_block_data(height, gas_used, capacity, 0, 0)?;

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
impl<L2, Metadata, DA> RunnableTask for GasPriceServiceV1<L2, Metadata, DA>
where
    L2: L2BlockSource,
    Metadata: MetadataStorage,
    DA: DaBlockCostsSource,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                tracing::debug!("Stopping gas price service");
                should_continue = false;
            }
            l2_block_res = self.l2_block_source.get_l2_block() => {
                tracing::info!("Received L2 block result: {:?}", l2_block_res);
                let block = l2_block_res?;

                tracing::debug!("Updating gas price algorithm");
                self.apply_block_info_to_gas_algorithm(block).await?;
                should_continue = true;
            }
            da_block_costs = self.da_source_channel.recv() => {
                tracing::info!("Received DA block costs: {:?}", da_block_costs);
                let da_block_costs = da_block_costs?;

                tracing::debug!("Updating DA block costs");
                self.algorithm_updater.update_da_record_data(da_block_costs.l2_block_range, da_block_costs.blob_cost_wei)?;
                should_continue = true;
            }
        }
        Ok(should_continue)
    }

    async fn shutdown(mut self) -> anyhow::Result<()> {
        // handle all the remaining l2 blocks
        while let Some(Ok(block)) = self.l2_block_source.get_l2_block().now_or_never() {
            tracing::debug!("Updating gas price algorithm");
            self.apply_block_info_to_gas_algorithm(block).await?;
        }

        while let Ok(da_block_costs) = self.da_source_channel.try_recv() {
            tracing::debug!("Updating DA block costs");
            self.algorithm_updater.update_da_record_data(
                da_block_costs.l2_block_range,
                da_block_costs.blob_cost_wei,
            )?;
        }

        // run shutdown hooks for internal services
        self.da_source_adapter_handle.shutdown().await?;

        Ok(())
    }
}

pub fn initialize_algorithm<Metadata>(
    config: &V1AlgorithmConfig,
    latest_block_height: u32,
    metadata_storage: &Metadata,
) -> crate::common::utils::Result<(AlgorithmUpdaterV1, SharedV1Algorithm)>
where
    Metadata: MetadataStorage,
{
    let algorithm_updater;
    if let Some(updater_metadata) = metadata_storage
        .get_metadata(&latest_block_height.into())
        .map_err(|err| {
            crate::common::utils::Error::CouldNotInitUpdater(anyhow::anyhow!(err))
        })?
    {
        if let Ok(v1_metadata) = V1Metadata::try_from(updater_metadata.clone()) {
            algorithm_updater = v1_algorithm_from_metadata(v1_metadata, config);
        } else {
            let v0_metadata = V0Metadata::try_from(updater_metadata).map_err(|_| {
                crate::common::utils::Error::CouldNotInitUpdater(anyhow::anyhow!(
                    "Could not convert metadata to V0Metadata"
                ))
            })?;
            let v1_metadata = V1Metadata::construct_from_v0_metadata(v0_metadata, config)
                .map_err(|err| {
                    crate::common::utils::Error::CouldNotInitUpdater(anyhow::anyhow!(err))
                })?;
            algorithm_updater = v1_algorithm_from_metadata(v1_metadata, config);
        }
    } else {
        algorithm_updater = AlgorithmUpdaterV1::from(config);
    }

    let shared_algo =
        SharedGasPriceAlgo::new_with_algorithm(algorithm_updater.algorithm());

    Ok((algorithm_updater, shared_algo))
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
        v1::{
            da_source_service::{
                dummy_costs::DummyDaBlockCosts,
                service::DaSourceService,
                DaBlockCosts,
            },
            metadata::V1AlgorithmConfig,
            service::{
                initialize_algorithm,
                GasPriceServiceV1,
            },
        },
    };
    use fuel_core_services::{
        RunnableTask,
        StateWatcher,
    };
    use fuel_core_types::fuel_types::BlockHeight;
    use futures::FutureExt;
    use std::{
        num::NonZeroU64,
        sync::Arc,
        time::Duration,
    };
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
    async fn run__updates_gas_price_with_l2_block_source() {
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
        let config = V1AlgorithmConfig {
            new_exec_gas_price: 100,
            min_exec_gas_price: 50,
            exec_gas_price_change_percent: 20,
            l2_block_fullness_threshold_percent: 20,
            gas_price_factor: NonZeroU64::new(10).unwrap(),
            min_da_gas_price: 10,
            max_da_gas_price_change_percent: 20,
            da_p_component: 4,
            da_d_component: 2,
            normal_range_size: 10,
            capped_range_size: 100,
            decrease_range_size: 4,
            block_activity_threshold: 20,
        };
        let (algo_updater, shared_algo) =
            initialize_algorithm(&config, l2_block_height, &metadata_storage).unwrap();

        let notifier = Arc::new(tokio::sync::Notify::new());
        let dummy_da_source = DaSourceService::new(
            DummyDaBlockCosts::new(
                Err(anyhow::anyhow!("unused at the moment")),
                notifier.clone(),
            ),
            None,
        );

        let mut service = GasPriceServiceV1::new(
            l2_block_source,
            metadata_storage,
            shared_algo,
            algo_updater,
            dummy_da_source,
        );
        let read_algo = service.next_block_algorithm();
        let mut watcher = StateWatcher::default();
        let initial_price = read_algo.next_gas_price();

        // when
        service.run(&mut watcher).await.unwrap();
        l2_block_sender.send(l2_block).await.unwrap();
        service.shutdown().await.unwrap();

        // then
        let actual_price = read_algo.next_gas_price();
        assert_ne!(initial_price, actual_price);
    }

    #[tokio::test]
    async fn run__updates_gas_price_with_da_block_cost_source() {
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
        let l2_block_height = 1;
        let config = V1AlgorithmConfig {
            new_exec_gas_price: 100,
            min_exec_gas_price: 50,
            exec_gas_price_change_percent: 20,
            l2_block_fullness_threshold_percent: 20,
            gas_price_factor: NonZeroU64::new(10).unwrap(),
            min_da_gas_price: 100,
            max_da_gas_price_change_percent: 50,
            da_p_component: 4,
            da_d_component: 2,
            normal_range_size: 10,
            capped_range_size: 100,
            decrease_range_size: 4,
            block_activity_threshold: 20,
        };
        let (algo_updater, shared_algo) =
            initialize_algorithm(&config, l2_block_height, &metadata_storage).unwrap();

        let notifier = Arc::new(tokio::sync::Notify::new());
        let da_source = DaSourceService::new(
            DummyDaBlockCosts::new(
                Ok(DaBlockCosts {
                    l2_block_range: 1..2,
                    blob_cost_wei: 9000,
                    blob_size_bytes: 3000,
                }),
                notifier.clone(),
            ),
            Some(Duration::from_millis(100)),
        );
        let mut watcher = StateWatcher::default();

        let mut service = GasPriceServiceV1::new(
            l2_block_source,
            metadata_storage,
            shared_algo,
            algo_updater,
            da_source,
        );
        let read_algo = service.next_block_algorithm();
        let initial_price = read_algo.next_gas_price();

        // the RunnableTask depends on the handle passed to it for the da block cost source to already be running,
        // which is the responsibility of the UninitializedTask in the `into_task` method of the RunnableService
        // here we mimic that behaviour by running the da block cost service.
        let notified = notifier.notified();
        service
            .da_source_adapter_handle
            .run(&mut watcher)
            .await
            .unwrap();

        // when
        service.run(&mut watcher).await.unwrap();
        l2_block_sender.send(l2_block).await.unwrap();
        service.shutdown().await.unwrap();
        let maybe_notified = tokio::time::timeout(Duration::from_secs(1), notified).await;

        // then
        let actual_price = read_algo.next_gas_price();
        // assert_ne!(initial_price, actual_price);
        assert!(maybe_notified.is_ok());
    }
}
