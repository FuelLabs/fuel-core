use std::num::NonZeroU64;

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
        GetDaSequenceNumber,
        GetMetadataStorage,
        SetDaSequenceNumber,
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
use tokio::sync::broadcast::Receiver;

/// The service that updates the gas price algorithm.
pub struct GasPriceServiceV1<L2, DA, StorageTxProvider> {
    /// The algorithm that can be used in the next block
    shared_algo: SharedV1Algorithm,
    /// The L2 block source
    l2_block_source: L2,
    /// The algorithm updater
    algorithm_updater: AlgorithmUpdaterV1,
    /// the da source adapter handle
    da_source_adapter_handle: DaSourceService<DA>,
    /// The da source channel
    da_source_channel: Receiver<DaBlockCosts>,
    /// Buffer of block costs from the DA chain
    da_block_costs_buffer: Vec<DaBlockCosts>,
    /// Storage transaction provider for metadata and unrecorded blocks
    storage_tx_provider: StorageTxProvider,
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
        tracing::info!("Received L2 block result: {:?}", l2_block_res);
        let block = l2_block_res?;

        tracing::debug!("Updating gas price algorithm");
        self.apply_block_info_to_gas_algorithm(block).await?;
        Ok(())
    }
}

impl<L2, DA, AtomicStorage> GasPriceServiceV1<L2, DA, AtomicStorage>
where
    DA: DaBlockCostsSource,
    AtomicStorage: GasPriceServiceAtomicStorage,
{
    pub fn new(
        l2_block_source: L2,
        shared_algo: SharedV1Algorithm,
        algorithm_updater: AlgorithmUpdaterV1,
        da_source_adapter_handle: DaSourceService<DA>,
        storage_tx_provider: AtomicStorage,
    ) -> Self {
        let da_source_channel =
            da_source_adapter_handle.shared_data().clone().subscribe();
        Self {
            shared_algo,
            l2_block_source,
            algorithm_updater,
            da_source_adapter_handle,
            da_source_channel,
            da_block_costs_buffer: Vec::new(),
            storage_tx_provider,
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

    async fn update(&mut self, new_algorithm: AlgorithmV1) {
        self.shared_algo.update(new_algorithm).await;
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
        let prev_height = height.saturating_sub(1);
        let mut sequence_number = storage_tx
            .get_sequence_number(&BlockHeight::from(prev_height))
            .map_err(|err| anyhow!(err))?;

        for da_block_costs in &self.da_block_costs_buffer {
            tracing::debug!("Updating DA block costs: {:?}", da_block_costs);
            self.algorithm_updater.update_da_record_data(
                &da_block_costs.l2_blocks,
                da_block_costs.bundle_size_bytes,
                da_block_costs.blob_cost_wei,
                &mut storage_tx.as_unrecorded_blocks(),
            )?;
            sequence_number = Some(da_block_costs.bundle_sequence_number);
        }

        if let Some(sequence_number) = sequence_number {
            storage_tx
                .set_sequence_number(&BlockHeight::from(height), sequence_number)
                .map_err(|err| anyhow!(err))?;
        }

        self.algorithm_updater.update_l2_block_data(
            height,
            gas_used,
            capacity,
            block_bytes,
            block_fees as u128,
            &mut storage_tx.as_unrecorded_blocks(),
        )?;

        let metadata = self.algorithm_updater.clone().into();
        storage_tx
            .set_metadata(&metadata)
            .map_err(|err| anyhow!(err))?;
        AtomicStorage::commit_transaction(storage_tx)?;
        let new_algo = self.algorithm_updater.algorithm();
        self.shared_algo.update(new_algo).await;
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
                self.shared_algo.update(new_algo).await;
            }
            BlockInfo::Block {
                height,
                gas_used,
                block_gas_capacity,
                block_bytes,
                block_fees,
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
        self.da_source_adapter_handle.shutdown().await?;

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
    latest_block_height: u32,
    metadata_storage: &Metadata,
) -> crate::common::utils::Result<(AlgorithmUpdaterV1, SharedV1Algorithm)>
where
    Metadata: GetMetadataStorage,
{
    let algorithm_updater = if let Some(updater_metadata) = metadata_storage
        .get_metadata(&latest_block_height.into())
        .map_err(|err| {
            crate::common::utils::Error::CouldNotInitUpdater(anyhow::anyhow!(err))
        })? {
        let v1_metadata = convert_to_v1_metadata(updater_metadata, config)?;
        v1_algorithm_from_metadata(v1_metadata, config)
    } else {
        updater_from_config(config)
    };

    let shared_algo =
        SharedGasPriceAlgo::new_with_algorithm(algorithm_updater.algorithm());

    Ok((algorithm_updater, shared_algo))
}

#[allow(clippy::arithmetic_side_effects)]
#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use std::{
        num::NonZeroU64,
        sync::Arc,
        time::Duration,
    };

    use tokio::sync::mpsc;

    use fuel_core_services::{
        RunnableTask,
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
                GasPriceColumn::UnrecordedBlocks,
                SequenceNumberTable,
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
            GetMetadataStorage,
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
            },
            service::{
                initialize_algorithm,
                GasPriceServiceV1,
            },
            uninitialized_task::fuel_storage_unrecorded_blocks::FuelStorageUnrecordedBlocks,
        },
    };

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
        let inner = database();
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
            shared_algo,
            algo_updater,
            dummy_da_source,
            inner,
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
        let block_height = 2;
        let l2_block_2 = BlockInfo::Block {
            height: block_height,
            gas_used: 60,
            block_gas_capacity: 100,
            block_bytes: 100,
            block_fees: 100,
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
            max_da_gas_price_change_percent: 100,
            da_p_component: 4,
            da_d_component: 2,
            normal_range_size: 10,
            capped_range_size: 100,
            decrease_range_size: 4,
            block_activity_threshold: 20,
        };
        let mut inner = database();
        let mut tx = inner.write_transaction();
        tx.storage_as_mut::<UnrecordedBlocksTable>()
            .insert(&BlockHeight::from(1), &100)
            .unwrap();
        tx.commit().unwrap();
        let mut algo_updater = updater_from_config(&config);
        let shared_algo =
            SharedGasPriceAlgo::new_with_algorithm(algo_updater.algorithm());
        algo_updater.l2_block_height = block_height - 1;
        algo_updater.last_profit = 10_000;
        algo_updater.new_scaled_da_gas_price = 10_000_000;

        let notifier = Arc::new(tokio::sync::Notify::new());
        let da_source = DaSourceService::new(
            DummyDaBlockCosts::new(
                Ok(DaBlockCosts {
                    bundle_sequence_number: 1,
                    l2_blocks: (1..2).collect(),
                    blob_cost_wei: 9000,
                    bundle_size_bytes: 3000,
                }),
                notifier.clone(),
            ),
            Some(Duration::from_millis(1)),
        );
        let mut watcher = StateWatcher::started();

        let mut service = GasPriceServiceV1::new(
            l2_block_source,
            shared_algo,
            algo_updater,
            da_source,
            inner,
        );
        let read_algo = service.next_block_algorithm();
        let initial_price = read_algo.next_gas_price();

        // the RunnableTask depends on the handle passed to it for the da block cost source to already be running,
        // which is the responsibility of the UninitializedTask in the `into_task` method of the RunnableService
        // here we mimic that behaviour by running the da block cost service.
        let mut da_source_watcher = StateWatcher::started();
        service
            .da_source_adapter_handle
            .run(&mut da_source_watcher)
            .await;

        service.run(&mut watcher).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        l2_block_sender.send(l2_block_2).await.unwrap();

        // when
        service.run(&mut watcher).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
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
            max_da_gas_price_change_percent: 100,
            da_p_component: 4,
            da_d_component: 2,
            normal_range_size: 10,
            capped_range_size: 100,
            decrease_range_size: 4,
            block_activity_threshold: 20,
        }
    }

    #[tokio::test]
    async fn run__responses_from_da_service_update_sequence_number_in_storage() {
        // given
        let sequence_number = 1234;
        let block_height = 2;
        let l2_block_2 = BlockInfo::Block {
            height: block_height,
            gas_used: 60,
            block_gas_capacity: 100,
            block_bytes: 100,
            block_fees: 100,
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
        let mut algo_updater = updater_from_config(&config);
        let shared_algo =
            SharedGasPriceAlgo::new_with_algorithm(algo_updater.algorithm());
        algo_updater.l2_block_height = block_height - 1;
        algo_updater.last_profit = 10_000;
        algo_updater.new_scaled_da_gas_price = 10_000_000;

        let notifier = Arc::new(tokio::sync::Notify::new());
        let da_source = DaSourceService::new(
            DummyDaBlockCosts::new(
                Ok(DaBlockCosts {
                    bundle_sequence_number: sequence_number,
                    l2_blocks: (1..2).collect(),
                    blob_cost_wei: 9000,
                    bundle_size_bytes: 3000,
                }),
                notifier.clone(),
            ),
            Some(Duration::from_millis(1)),
        );
        let mut watcher = StateWatcher::started();

        let mut service = GasPriceServiceV1::new(
            l2_block_source,
            shared_algo,
            algo_updater,
            da_source,
            inner,
        );
        let read_algo = service.next_block_algorithm();
        let initial_price = read_algo.next_gas_price();

        // the RunnableTask depends on the handle passed to it for the da block cost source to already be running,
        // which is the responsibility of the UninitializedTask in the `into_task` method of the RunnableService
        // here we mimic that behaviour by running the da block cost service.
        let mut da_source_watcher = StateWatcher::started();
        service
            .da_source_adapter_handle
            .run(&mut da_source_watcher)
            .await;

        service.run(&mut watcher).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        l2_block_sender.send(l2_block_2).await.unwrap();

        // when
        service.run(&mut watcher).await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        // then
        let latest_sequence_number = service
            .storage_tx_provider
            .storage::<SequenceNumberTable>()
            .get(&BlockHeight::from(block_height))
            .unwrap()
            .unwrap();
        assert_eq!(*latest_sequence_number, sequence_number);

        service.shutdown().await.unwrap();
    }
}
