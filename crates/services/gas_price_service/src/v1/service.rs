use crate::{
    common::{
        gas_price_algorithm::SharedGasPriceAlgo,
        l2_block_source::L2BlockSource,
        updater_metadata::UpdaterMetadata,
        utils::{
            BlockInfo,
            Result as GasPriceResult,
        },
    },
    ports::{
        GasPriceServiceAtomicStorage,
        GetLatestRecordedHeight,
        GetMetadataStorage,
        SetLatestRecordedHeight,
        SetMetadataStorage,
    },
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
            updater_from_config,
            v1_algorithm_from_metadata,
            V1AlgorithmConfig,
            V1Metadata,
        },
        uninitialized_task::fuel_storage_unrecorded_blocks::{
            AsUnrecordedBlocks,
            FuelStorageUnrecordedBlocks,
        },
    },
};
use anyhow::anyhow;
use async_trait::async_trait;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    Service,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::fuel_types::BlockHeight;
use fuel_gas_price_algorithm::{
    v0::AlgorithmUpdaterV0,
    v1::{
        AlgorithmUpdaterV1,
        AlgorithmV1,
        UnrecordedBlocks,
    },
};
use futures::FutureExt;
use std::{
    num::NonZeroU64,
    sync::{
        atomic::{
            AtomicU32,
            Ordering,
        },
        Arc,
        Mutex,
    },
};
use tokio::sync::broadcast::Receiver;

#[derive(Debug)]
pub struct LatestGasPrice<Height, GasPrice> {
    inner: Arc<parking_lot::RwLock<(Height, GasPrice)>>,
}

impl<Height, GasPrice> Clone for LatestGasPrice<Height, GasPrice> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<Height, GasPrice> LatestGasPrice<Height, GasPrice> {
    pub fn new(height: Height, price: GasPrice) -> Self {
        let pair = (height, price);
        let inner = Arc::new(parking_lot::RwLock::new(pair));
        Self { inner }
    }

    pub fn set(&mut self, height: Height, price: GasPrice) {
        *self.inner.write() = (height, price);
    }
}

impl<Height: Copy, GasPrice: Copy> LatestGasPrice<Height, GasPrice> {
    pub fn get(&self) -> (Height, GasPrice) {
        *self.inner.read()
    }
}

/// The service that updates the gas price algorithm.
pub struct GasPriceServiceV1<L2, DA, AtomicStorage>
where
    DA: DaBlockCostsSource + 'static,
{
    /// The algorithm that can be used in the next block
    shared_algo: SharedV1Algorithm,
    /// The latest gas price
    latest_gas_price: LatestGasPrice<u32, u64>,
    /// The L2 block source
    l2_block_source: L2,
    /// The algorithm updater
    algorithm_updater: AlgorithmUpdaterV1,
    /// the da source adapter handle
    da_source_adapter_handle: ServiceRunner<DaSourceService<DA>>,
    /// The da source channel
    da_source_channel: Receiver<DaBlockCosts>,
    /// Buffer of block costs from the DA chain
    da_block_costs_buffer: Vec<DaBlockCosts>,
    /// Storage transaction provider for metadata and unrecorded blocks
    storage_tx_provider: AtomicStorage,
    /// communicates to the Da source service what the latest L2 block was
    latest_l2_block: Arc<AtomicU32>,
    /// Initial Recorded Height
    initial_recorded_height: Option<BlockHeight>,
}

impl<L2, DA, StorageTxProvider> GasPriceServiceV1<L2, DA, StorageTxProvider>
where
    DA: DaBlockCostsSource + 'static,
{
    pub(crate) fn update_latest_gas_price(&mut self, block_info: &BlockInfo) {
        match block_info {
            BlockInfo::GenesisBlock => {
                // do nothing
            }
            BlockInfo::Block {
                height, gas_price, ..
            } => {
                self.latest_gas_price.set(*height, *gas_price);
            }
        }
    }

    #[cfg(test)]
    pub fn latest_l2_block(&self) -> &AtomicU32 {
        &self.latest_l2_block
    }

    #[cfg(test)]
    pub fn initial_recorded_height(&self) -> Option<BlockHeight> {
        self.initial_recorded_height
    }
}

impl<L2, DA, AtomicStorage> GasPriceServiceV1<L2, DA, AtomicStorage>
where
    L2: L2BlockSource,
    DA: DaBlockCostsSource,
    AtomicStorage: GasPriceServiceAtomicStorage,
{
    async fn commit_block_data_to_algorithm(
        &mut self,
        l2_block_res: GasPriceResult<BlockInfo>,
    ) -> anyhow::Result<()> {
        tracing::debug!("Received L2 block result: {:?}", l2_block_res);
        let block = l2_block_res?;

        self.update_latest_gas_price(&block);
        tracing::debug!("Updating gas price algorithm");
        self.apply_block_info_to_gas_algorithm(block).await?;

        self.notify_da_source_service_l2_block(block)?;
        Ok(())
    }

    fn notify_da_source_service_l2_block(&self, block: BlockInfo) -> anyhow::Result<()> {
        tracing::debug!("Notifying the Da source service of the latest L2 block");
        match block {
            BlockInfo::GenesisBlock => {}
            BlockInfo::Block { height, .. } => {
                self.latest_l2_block.store(height, Ordering::Release);
            }
        }
        Ok(())
    }
}

impl<L2, DA, AtomicStorage> GasPriceServiceV1<L2, DA, AtomicStorage>
where
    DA: DaBlockCostsSource,
    AtomicStorage: GasPriceServiceAtomicStorage,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        l2_block_source: L2,
        shared_algo: SharedV1Algorithm,
        latest_gas_price: LatestGasPrice<u32, u64>,
        algorithm_updater: AlgorithmUpdaterV1,
        da_source_adapter_handle: ServiceRunner<DaSourceService<DA>>,
        storage_tx_provider: AtomicStorage,
        latest_l2_block: Arc<AtomicU32>,
        initial_recorded_height: Option<BlockHeight>,
    ) -> Self {
        let da_source_channel = da_source_adapter_handle.shared.clone().subscribe();
        Self {
            shared_algo,
            latest_gas_price,
            l2_block_source,
            algorithm_updater,
            da_source_adapter_handle,
            da_source_channel,
            da_block_costs_buffer: Vec::new(),
            storage_tx_provider,
            latest_l2_block,
            initial_recorded_height,
        }
    }

    pub fn algorithm_updater(&self) -> &AlgorithmUpdaterV1 {
        &self.algorithm_updater
    }

    pub fn next_block_algorithm(&self) -> SharedV1Algorithm {
        self.shared_algo.clone()
    }

    #[cfg(test)]
    pub fn storage_tx_provider(&self) -> &AtomicStorage {
        &self.storage_tx_provider
    }

    fn update(&mut self, new_algorithm: AlgorithmV1) {
        self.shared_algo.update(new_algorithm);
    }

    fn validate_block_gas_capacity(
        block_gas_capacity: u64,
    ) -> anyhow::Result<NonZeroU64> {
        NonZeroU64::new(block_gas_capacity)
            .ok_or_else(|| anyhow!("Block gas capacity must be non-zero"))
    }

    async fn handle_normal_block(
        &mut self,
        height: u32,
        gas_used: u64,
        block_gas_capacity: u64,
        block_bytes: u64,
        block_fees: u64,
    ) -> anyhow::Result<()> {
        let capacity = Self::validate_block_gas_capacity(block_gas_capacity)?;
        let mut storage_tx = self.storage_tx_provider.begin_transaction()?;
        let mut new_recorded_height = match storage_tx
            .get_recorded_height()
            .map_err(|err| anyhow!(err))?
        {
            Some(_) => None,
            None => {
                // Sets it on first run
                self.initial_recorded_height.take()
            }
        };

        for da_block_costs in &self.da_block_costs_buffer {
            tracing::debug!("Updating DA block costs: {:?}", da_block_costs);
            let l2_blocks = da_block_costs.l2_blocks.clone();
            let end = *l2_blocks.end();
            self.algorithm_updater.update_da_record_data(
                l2_blocks,
                da_block_costs.bundle_size_bytes,
                da_block_costs.blob_cost_wei,
                &mut storage_tx.as_unrecorded_blocks(),
            )?;
            new_recorded_height = Some(BlockHeight::from(end));
        }

        if let Some(recorded_height) = new_recorded_height {
            tracing::debug!("Updating recorded height to {:?}", recorded_height);
            storage_tx
                .set_recorded_height(recorded_height)
                .map_err(|err| anyhow!(err))?;
        }

        let fee_in_wei = u128::from(block_fees).saturating_mul(1_000_000_000);
        self.algorithm_updater.update_l2_block_data(
            height,
            gas_used,
            capacity,
            block_bytes,
            fee_in_wei,
            &mut storage_tx.as_unrecorded_blocks(),
        )?;

        let metadata = self.algorithm_updater.clone().into();
        tracing::debug!("Setting metadata: {:?}", metadata);
        storage_tx
            .set_metadata(&metadata)
            .map_err(|err| anyhow!(err))?;
        AtomicStorage::commit_transaction(storage_tx)?;
        let new_algo = self.algorithm_updater.algorithm();
        tracing::debug!("Updating gas price: {}", &new_algo.calculate());
        self.shared_algo.update(new_algo);
        // Clear the buffer after committing changes
        self.da_block_costs_buffer.clear();
        Ok(())
    }

    async fn apply_block_info_to_gas_algorithm(
        &mut self,
        l2_block: BlockInfo,
    ) -> anyhow::Result<()> {
        match l2_block {
            BlockInfo::GenesisBlock => {
                let metadata: UpdaterMetadata = self.algorithm_updater.clone().into();
                let mut tx = self.storage_tx_provider.begin_transaction()?;
                tx.set_metadata(&metadata).map_err(|err| anyhow!(err))?;
                AtomicStorage::commit_transaction(tx)?;
                let new_algo = self.algorithm_updater.algorithm();
                self.shared_algo.update(new_algo);
            }
            BlockInfo::Block {
                height,
                gas_used,
                block_gas_capacity,
                block_bytes,
                block_fees,
                ..
            } => {
                self.handle_normal_block(
                    height,
                    gas_used,
                    block_gas_capacity,
                    block_bytes,
                    block_fees,
                )
                .await?;
            }
        }

        Ok(())
    }
}

#[async_trait]
impl<L2, DA, AtomicStorage> RunnableTask for GasPriceServiceV1<L2, DA, AtomicStorage>
where
    L2: L2BlockSource,
    DA: DaBlockCostsSource,
    AtomicStorage: GasPriceServiceAtomicStorage,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                tracing::debug!("Stopping gas price service");
                TaskNextAction::Stop
            }
            l2_block_res = self.l2_block_source.get_l2_block() => {
                tracing::debug!("Received L2 block result: {:?}", l2_block_res);
                let res = self.commit_block_data_to_algorithm(l2_block_res).await;
                TaskNextAction::always_continue(res)
            }
            da_block_costs_res = self.da_source_channel.recv() => {
                tracing::debug!("Received DA block costs: {:?}", da_block_costs_res);
                match da_block_costs_res {
                    Ok(da_block_costs) => {
                        self.da_block_costs_buffer.push(da_block_costs);
                        TaskNextAction::Continue
                    },
                    Err(err) => {
                        let err = anyhow!("Error receiving DA block costs: {:?}", err);
                        TaskNextAction::ErrorContinue(err)
                    }
                }
            }
        }
    }

    async fn shutdown(mut self) -> anyhow::Result<()> {
        // handle all the remaining l2 blocks
        while let Some(Ok(block)) = self.l2_block_source.get_l2_block().now_or_never() {
            tracing::debug!("Updating gas price algorithm before shutdown");
            self.apply_block_info_to_gas_algorithm(block).await?;
        }

        // run shutdown hooks for internal services
        self.da_source_adapter_handle.stop_and_await().await?;

        Ok(())
    }
}

fn convert_to_v1_metadata(
    updater_metadata: UpdaterMetadata,
    config: &V1AlgorithmConfig,
) -> crate::common::utils::Result<V1Metadata> {
    if let Ok(v1_metadata) = V1Metadata::try_from(updater_metadata.clone()) {
        Ok(v1_metadata)
    } else {
        let v0_metadata = V0Metadata::try_from(updater_metadata).map_err(|_| {
            crate::common::utils::Error::CouldNotInitUpdater(anyhow::anyhow!(
                "Could not convert metadata to V0Metadata"
            ))
        })?;
        V1Metadata::construct_from_v0_metadata(v0_metadata, config).map_err(|err| {
            crate::common::utils::Error::CouldNotInitUpdater(anyhow::anyhow!(err))
        })
    }
}

pub fn initialize_algorithm<Metadata>(
    config: &V1AlgorithmConfig,
    latest_metadata_block_height: u32,
    latest_l2_block_height: u32,
    metadata_storage: &Metadata,
) -> crate::common::utils::Result<(AlgorithmUpdaterV1, SharedV1Algorithm)>
where
    Metadata: GetMetadataStorage,
{
    let algorithm_updater = if let Some(updater_metadata) = metadata_storage
        .get_metadata(&latest_metadata_block_height.into())
        .map_err(|err| {
            crate::common::utils::Error::CouldNotInitUpdater(anyhow::anyhow!(err))
        })? {
        let v1_metadata = convert_to_v1_metadata(updater_metadata, config)?;
        v1_algorithm_from_metadata(v1_metadata, config)
    } else {
        updater_from_config(config, latest_l2_block_height)
    };

    let shared_algo =
        SharedGasPriceAlgo::new_with_algorithm(algorithm_updater.algorithm());

    Ok((algorithm_updater, shared_algo))
}

#[allow(clippy::arithmetic_side_effects)]
#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        num::NonZeroU64,
        sync::{
            atomic::AtomicU32,
            Arc,
        },
        time::Duration,
    };
    use tokio::sync::mpsc;

    use fuel_core_services::{
        RunnableTask,
        Service,
        ServiceRunner,
        StateWatcher,
    };
    use fuel_core_storage::{
        structured_storage::test::InMemoryStorage,
        transactional::{
            IntoTransaction,
            StorageTransaction,
            WriteTransaction,
        },
        StorageAsMut,
    };
    use fuel_core_types::fuel_types::BlockHeight;

    use crate::{
        common::{
            fuel_core_storage_adapter::storage::{
                GasPriceColumn,
                GasPriceMetadata,
                UnrecordedBlocksTable,
            },
            gas_price_algorithm::SharedGasPriceAlgo,
            l2_block_source::L2BlockSource,
            updater_metadata::UpdaterMetadata,
            utils::{
                BlockInfo,
                Result as GasPriceResult,
            },
        },
        ports::{
            GasPriceServiceAtomicStorage,
            GetLatestRecordedHeight,
            GetMetadataStorage,
            SetLatestRecordedHeight,
            SetMetadataStorage,
        },
        v1::{
            da_source_service::{
                dummy_costs::DummyDaBlockCosts,
                service::DaSourceService,
                DaBlockCosts,
            },
            metadata::{
                updater_from_config,
                V1AlgorithmConfig,
                V1Metadata,
            },
            service::{
                initialize_algorithm,
                GasPriceServiceV1,
                LatestGasPrice,
            },
            uninitialized_task::fuel_storage_unrecorded_blocks::AsUnrecordedBlocks,
        },
    };
    use fuel_gas_price_algorithm::v1::{
        Bytes,
        Height,
    };

    struct FakeL2BlockSource {
        l2_block: mpsc::Receiver<BlockInfo>,
    }

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

    impl SetMetadataStorage for FakeMetadata {
        fn set_metadata(&mut self, metadata: &UpdaterMetadata) -> GasPriceResult<()> {
            *self.inner.lock().unwrap() = Some(metadata.clone());
            Ok(())
        }
    }

    impl GetMetadataStorage for FakeMetadata {
        fn get_metadata(
            &self,
            _: &BlockHeight,
        ) -> GasPriceResult<Option<UpdaterMetadata>> {
            let metadata = self.inner.lock().unwrap().clone();
            Ok(metadata)
        }
    }

    fn database() -> StorageTransaction<InMemoryStorage<GasPriceColumn>> {
        InMemoryStorage::default().into_transaction()
    }

    #[tokio::test]
    async fn run__updates_gas_price_with_l2_block_source() {
        // given
        let block_height = 1;
        let l2_block = BlockInfo::Block {
            height: block_height,
            gas_used: 60,
            block_gas_capacity: 100,
            block_bytes: 100,
            block_fees: 100,
            gas_price: 100,
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
            max_da_gas_price: 11,
            max_da_gas_price_change_percent: 20,
            da_p_component: 4,
            da_d_component: 2,
            normal_range_size: 10,
            capped_range_size: 100,
            decrease_range_size: 4,
            block_activity_threshold: 20,
            da_poll_interval: None,
            starting_recorded_height: None,
        };
        let inner = database();
        let (algo_updater, shared_algo) = initialize_algorithm(
            &config,
            l2_block_height,
            l2_block_height,
            &metadata_storage,
        )
        .unwrap();

        let notifier = Arc::new(tokio::sync::Notify::new());
        let latest_l2_height = Arc::new(AtomicU32::new(0));
        let recorded_height = BlockHeight::new(0);

        let dummy_da_source = DaSourceService::new(
            DummyDaBlockCosts::new(
                Err(anyhow::anyhow!("unused at the moment")),
                notifier.clone(),
            ),
            None,
            Arc::clone(&latest_l2_height),
            recorded_height,
        );
        let da_service_runner = ServiceRunner::new(dummy_da_source);
        da_service_runner.start_and_await().await.unwrap();
        let latest_gas_price = LatestGasPrice::new(0, 0);

        let mut service = GasPriceServiceV1::new(
            l2_block_source,
            shared_algo,
            latest_gas_price,
            algo_updater,
            da_service_runner,
            inner,
            latest_l2_height,
            None,
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

    #[tokio::test]
    async fn run__updates_gas_price_with_da_block_cost_source() {
        // given
        let block_height = 3;
        let l2_block_2 = BlockInfo::Block {
            height: block_height,
            gas_used: 60,
            block_gas_capacity: 100,
            block_bytes: 100,
            block_fees: 100,
            gas_price: 100,
        };

        let (l2_block_sender, l2_block_receiver) = mpsc::channel(1);
        let l2_block_source = FakeL2BlockSource {
            l2_block: l2_block_receiver,
        };

        let metadata_storage = FakeMetadata::empty();
        // Configured so exec gas price doesn't change, only da gas price
        let config = V1AlgorithmConfig {
            new_exec_gas_price: 100,
            min_exec_gas_price: 50,
            exec_gas_price_change_percent: 0,
            l2_block_fullness_threshold_percent: 20,
            gas_price_factor: NonZeroU64::new(10).unwrap(),
            min_da_gas_price: 0,
            max_da_gas_price: 1,
            max_da_gas_price_change_percent: 100,
            da_p_component: 4,
            da_d_component: 2,
            normal_range_size: 10,
            capped_range_size: 100,
            decrease_range_size: 4,
            block_activity_threshold: 20,
            da_poll_interval: None,
            starting_recorded_height: None,
        };
        let mut inner = database();
        let mut tx = inner.write_transaction();
        tx.storage_as_mut::<UnrecordedBlocksTable>()
            .insert(&BlockHeight::from(1), &100)
            .unwrap();
        tx.commit().unwrap();
        let mut algo_updater = updater_from_config(&config, 0);
        let shared_algo =
            SharedGasPriceAlgo::new_with_algorithm(algo_updater.algorithm());
        algo_updater.l2_block_height = block_height - 1;
        algo_updater.last_profit = 10_000;
        algo_updater.new_scaled_da_gas_price = 10_000_000;

        let latest_l2_block = Arc::new(AtomicU32::new(block_height - 1));
        let notifier = Arc::new(tokio::sync::Notify::new());
        let recorded_height = BlockHeight::new(0);
        let da_source = DaSourceService::new(
            DummyDaBlockCosts::new(
                Ok(DaBlockCosts {
                    bundle_id: 1,
                    l2_blocks: 1..=1,
                    blob_cost_wei: u128::MAX, // Very expensive to trigger a change
                    bundle_size_bytes: 3000,
                }),
                notifier.clone(),
            ),
            Some(Duration::from_millis(1)),
            Arc::clone(&latest_l2_block),
            recorded_height,
        );
        let mut watcher = StateWatcher::started();
        let da_service_runner = ServiceRunner::new(da_source);
        da_service_runner.start_and_await().await.unwrap();
        let latest_gas_price = LatestGasPrice::new(0, 0);

        let mut service = GasPriceServiceV1::new(
            l2_block_source,
            shared_algo,
            latest_gas_price,
            algo_updater,
            da_service_runner,
            inner,
            latest_l2_block,
            None,
        );
        let read_algo = service.next_block_algorithm();
        let initial_price = read_algo.next_gas_price();

        let next = service.run(&mut watcher).await;
        tokio::time::sleep(Duration::from_millis(3)).await;
        l2_block_sender.send(l2_block_2).await.unwrap();

        // when
        let next = service.run(&mut watcher).await;
        tokio::time::sleep(Duration::from_millis(3)).await;
        service.shutdown().await.unwrap();

        // then
        let actual_price = read_algo.next_gas_price();
        assert_ne!(initial_price, actual_price);
    }

    fn arbitrary_v1_algorithm_config() -> V1AlgorithmConfig {
        V1AlgorithmConfig {
            new_exec_gas_price: 100,
            min_exec_gas_price: 50,
            exec_gas_price_change_percent: 0,
            l2_block_fullness_threshold_percent: 20,
            gas_price_factor: NonZeroU64::new(10).unwrap(),
            min_da_gas_price: 0,
            max_da_gas_price: 1,
            max_da_gas_price_change_percent: 100,
            da_p_component: 4,
            da_d_component: 2,
            normal_range_size: 10,
            capped_range_size: 100,
            decrease_range_size: 4,
            block_activity_threshold: 20,
            da_poll_interval: None,
            starting_recorded_height: None,
        }
    }

    #[tokio::test]
    async fn run__responses_from_da_service_update_recorded_height_in_storage() {
        // given
        let recorded_block_height = 100;
        let block_height = 200;
        let l2_block = BlockInfo::Block {
            height: block_height,
            gas_used: 60,
            block_gas_capacity: 100,
            block_bytes: 100,
            block_fees: 100,
            gas_price: 100,
        };

        let (l2_block_sender, l2_block_receiver) = mpsc::channel(1);
        let l2_block_source = FakeL2BlockSource {
            l2_block: l2_block_receiver,
        };

        let metadata_storage = FakeMetadata::empty();
        // Configured so exec gas price doesn't change, only da gas price
        let config = arbitrary_v1_algorithm_config();
        let mut inner = database();
        let mut tx = inner.write_transaction();
        tx.storage_as_mut::<UnrecordedBlocksTable>()
            .insert(&BlockHeight::from(1), &100)
            .unwrap();
        tx.commit().unwrap();
        let mut algo_updater = updater_from_config(&config, 0);
        let shared_algo =
            SharedGasPriceAlgo::new_with_algorithm(algo_updater.algorithm());
        algo_updater.l2_block_height = block_height - 1;
        algo_updater.last_profit = 10_000;
        algo_updater.new_scaled_da_gas_price = 10_000_000;

        let latest_l2_height = Arc::new(AtomicU32::new(block_height - 1));
        let notifier = Arc::new(tokio::sync::Notify::new());
        let recorded_height = BlockHeight::new(0);
        let da_source = DaSourceService::new(
            DummyDaBlockCosts::new(
                Ok(DaBlockCosts {
                    bundle_id: 8765,
                    l2_blocks: 1..=recorded_block_height,
                    blob_cost_wei: 9000,
                    bundle_size_bytes: 3000,
                }),
                notifier.clone(),
            ),
            Some(Duration::from_millis(1)),
            Arc::clone(&latest_l2_height),
            recorded_height,
        );
        let mut watcher = StateWatcher::started();
        let da_service_runner = ServiceRunner::new(da_source);
        da_service_runner.start_and_await().await.unwrap();
        let latest_gas_price = LatestGasPrice::new(0, 0);

        let mut service = GasPriceServiceV1::new(
            l2_block_source,
            shared_algo,
            latest_gas_price,
            algo_updater,
            da_service_runner,
            inner,
            latest_l2_height,
            None,
        );
        let read_algo = service.next_block_algorithm();
        let initial_price = read_algo.next_gas_price();

        service.run(&mut watcher).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        l2_block_sender.send(l2_block).await.unwrap();

        // when
        service.run(&mut watcher).await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        // then
        let latest_recorded_block_height = service
            .storage_tx_provider
            .get_recorded_height()
            .unwrap()
            .unwrap();
        assert_eq!(
            latest_recorded_block_height,
            BlockHeight::from(recorded_block_height)
        );

        service.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn run__stores_correct_amount_for_costs() {
        // given
        let recorded_block_height = 100;
        let block_height = 200;
        let l2_block = BlockInfo::Block {
            height: block_height,
            gas_used: 60,
            block_gas_capacity: 100,
            block_bytes: 100,
            block_fees: 100,
            gas_price: 0,
        };

        let (l2_block_sender, l2_block_receiver) = mpsc::channel(1);
        let l2_block_source = FakeL2BlockSource {
            l2_block: l2_block_receiver,
        };

        let metadata_storage = FakeMetadata::empty();
        // Configured so exec gas price doesn't change, only da gas price
        let config = arbitrary_v1_algorithm_config();
        let mut inner = database();
        let mut tx = inner.write_transaction();
        tx.storage_as_mut::<UnrecordedBlocksTable>()
            .insert(&BlockHeight::from(1), &100)
            .unwrap();
        tx.commit().unwrap();
        let mut algo_updater = updater_from_config(&config, 0);
        let shared_algo =
            SharedGasPriceAlgo::new_with_algorithm(algo_updater.algorithm());
        algo_updater.l2_block_height = block_height - 1;
        algo_updater.last_profit = 10_000;
        algo_updater.new_scaled_da_gas_price = 10_000_000;

        let notifier = Arc::new(tokio::sync::Notify::new());
        let blob_cost_wei = 9000;
        let latest_l2_height = Arc::new(AtomicU32::new(block_height - 1));
        let recorded_height = BlockHeight::new(0);
        let da_source = DaSourceService::new(
            DummyDaBlockCosts::new(
                Ok(DaBlockCosts {
                    bundle_id: 8765,
                    l2_blocks: 1..=recorded_block_height,
                    blob_cost_wei,
                    bundle_size_bytes: 3000,
                }),
                notifier.clone(),
            ),
            Some(Duration::from_millis(1)),
            Arc::clone(&latest_l2_height),
            recorded_height,
        );
        let mut watcher = StateWatcher::started();
        let da_service_runner = ServiceRunner::new(da_source);
        da_service_runner.start_and_await().await.unwrap();
        let latest_gas_price = LatestGasPrice::new(0, 0);

        let mut service = GasPriceServiceV1::new(
            l2_block_source,
            shared_algo,
            latest_gas_price,
            algo_updater,
            da_service_runner,
            inner,
            latest_l2_height,
            None,
        );
        let read_algo = service.next_block_algorithm();
        let initial_price = read_algo.next_gas_price();

        service.run(&mut watcher).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        l2_block_sender.send(l2_block).await.unwrap();

        // when
        service.run(&mut watcher).await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        // then
        let metadata: V1Metadata = service
            .storage_tx_provider
            .storage::<GasPriceMetadata>()
            .get(&block_height.into())
            .unwrap()
            .and_then(|x| x.v1().cloned())
            .unwrap();
        assert_eq!(metadata.latest_known_total_da_cost, blob_cost_wei);

        service.shutdown().await.unwrap();
    }

    fn arbitrary_config() -> V1AlgorithmConfig {
        V1AlgorithmConfig {
            new_exec_gas_price: 100,
            min_exec_gas_price: 50,
            exec_gas_price_change_percent: 20,
            l2_block_fullness_threshold_percent: 20,
            gas_price_factor: NonZeroU64::new(10).unwrap(),
            min_da_gas_price: 10,
            max_da_gas_price: 11,
            max_da_gas_price_change_percent: 20,
            da_p_component: 4,
            da_d_component: 2,
            normal_range_size: 10,
            capped_range_size: 100,
            decrease_range_size: 4,
            block_activity_threshold: 20,
            da_poll_interval: None,
            starting_recorded_height: None,
        }
    }

    #[derive(Clone)]
    struct FakeAtomicStorage {
        inner: Arc<Mutex<Option<BlockHeight>>>,
    }

    impl FakeAtomicStorage {
        fn new() -> Self {
            Self {
                inner: Arc::new(Mutex::new(None)),
            }
        }
    }

    impl GasPriceServiceAtomicStorage for FakeAtomicStorage {
        type Transaction<'a>
            = Self
        where
            Self: 'a;

        fn begin_transaction(&mut self) -> GasPriceResult<Self::Transaction<'_>> {
            Ok(self.clone())
        }

        fn commit_transaction(transaction: Self::Transaction<'_>) -> GasPriceResult<()> {
            Ok(())
        }
    }

    impl GetLatestRecordedHeight for FakeAtomicStorage {
        fn get_recorded_height(&self) -> GasPriceResult<Option<BlockHeight>> {
            let height = self.inner.lock().unwrap();
            Ok(*height)
        }
    }

    impl SetLatestRecordedHeight for FakeAtomicStorage {
        fn set_recorded_height(&mut self, height: BlockHeight) -> GasPriceResult<()> {
            *self.inner.lock().unwrap() = Some(height);
            Ok(())
        }
    }

    impl AsUnrecordedBlocks for FakeAtomicStorage {
        type Wrapper<'a> = OkUnrecordedBlocks;

        fn as_unrecorded_blocks(&mut self) -> Self::Wrapper<'_> {
            OkUnrecordedBlocks
        }
    }

    struct OkUnrecordedBlocks;

    impl UnrecordedBlocks for OkUnrecordedBlocks {
        fn insert(&mut self, height: Height, bytes: Bytes) -> Result<(), String> {
            Ok(())
        }

        fn remove(&mut self, height: &Height) -> Result<Option<Bytes>, String> {
            Ok(None)
        }
    }

    impl GetMetadataStorage for FakeAtomicStorage {
        fn get_metadata(
            &self,
            _: &BlockHeight,
        ) -> GasPriceResult<Option<UpdaterMetadata>> {
            Ok(None)
        }
    }

    impl SetMetadataStorage for FakeAtomicStorage {
        fn set_metadata(&mut self, _: &UpdaterMetadata) -> GasPriceResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn run__sets_the_latest_recorded_block_if_not_set() {
        // given
        let expected_recorded_height = BlockHeight::new(9999999);

        let block_height = 1;
        let l2_block = BlockInfo::Block {
            height: block_height,
            gas_used: 60,
            block_gas_capacity: 100,
            block_bytes: 100,
            block_fees: 100,
            gas_price: 100,
        };

        let (l2_block_sender, l2_block_receiver) = mpsc::channel(1);
        let l2_block_source = FakeL2BlockSource {
            l2_block: l2_block_receiver,
        };

        let metadata_storage = FakeMetadata::empty();
        let l2_block_height = 0;
        let config = arbitrary_config();
        let atomic_storage = FakeAtomicStorage::new();
        let handle = atomic_storage.clone();
        let (algo_updater, shared_algo) = initialize_algorithm(
            &config,
            l2_block_height,
            l2_block_height,
            &metadata_storage,
        )
        .unwrap();

        let notifier = Arc::new(tokio::sync::Notify::new());
        let latest_l2_height = Arc::new(AtomicU32::new(0));
        let recorded_height = BlockHeight::new(0);

        let dummy_da_source = DaSourceService::new(
            DummyDaBlockCosts::new(
                Err(anyhow::anyhow!("unused at the moment")),
                notifier.clone(),
            ),
            None,
            Arc::clone(&latest_l2_height),
            recorded_height,
        );
        let da_service_runner = ServiceRunner::new(dummy_da_source);
        da_service_runner.start_and_await().await.unwrap();
        let latest_gas_price = LatestGasPrice::new(0, 0);

        let mut service = GasPriceServiceV1::new(
            l2_block_source,
            shared_algo,
            latest_gas_price,
            algo_updater,
            da_service_runner,
            atomic_storage,
            latest_l2_height,
            Some(expected_recorded_height),
        );
        let read_algo = service.next_block_algorithm();
        let initial_price = read_algo.next_gas_price();
        let mut watcher = StateWatcher::default();

        // when
        service.run(&mut watcher).await;
        l2_block_sender.send(l2_block).await.unwrap();
        service.shutdown().await.unwrap();

        // then
        let actual = handle.get_recorded_height().unwrap().unwrap();

        assert_eq!(expected_recorded_height, actual);
    }
}
