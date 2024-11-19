#![allow(non_snake_case)]
use crate::{
    common::{
        fuel_core_storage_adapter::{
            GasPriceSettings,
            GasPriceSettingsProvider,
        },
        l2_block_source::L2BlockSource,
        updater_metadata::UpdaterMetadata,
        utils::{
            BlockInfo,
            Error as GasPriceError,
            Result as GasPriceResult,
        },
    },
    ports::{
        GasPriceData,
        L2Data,
        MetadataStorage,
    },
    v1::{
        da_source_service::{
            service::{
                DaBlockCostsSource,
                DaSourceService,
            },
            DaBlockCosts,
        },
        metadata::{
            V1AlgorithmConfig,
            V1Metadata,
        },
        service::GasPriceServiceV1,
        uninitialized_task::{
            initialize_algorithm,
            UninitializedTask,
        },
    },
};
use anyhow::anyhow;
use fuel_core_services::{
    stream::{
        BoxStream,
        IntoBoxStream,
    },
    RunnableTask,
    StateWatcher,
};
use fuel_core_storage::{
    transactional::AtomicView,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        header::ConsensusParametersVersion,
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
    services::block_importer::{
        ImportResult,
        SharedImportResult,
    },
};
use std::{
    num::{
        NonZero,
        NonZeroU64,
    },
    ops::Deref,
    sync::Arc,
    time::Duration,
};
use tokio::sync::mpsc::Receiver;

struct FakeL2BlockSource {
    l2_block: Receiver<BlockInfo>,
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
    fn get_metadata(&self, _: &BlockHeight) -> GasPriceResult<Option<UpdaterMetadata>> {
        let metadata = self.inner.lock().unwrap().clone();
        Ok(metadata)
    }

    fn set_metadata(&mut self, metadata: &UpdaterMetadata) -> GasPriceResult<()> {
        *self.inner.lock().unwrap() = Some(metadata.clone());
        Ok(())
    }
}

struct ErroringMetadata;

impl MetadataStorage for ErroringMetadata {
    fn get_metadata(&self, _: &BlockHeight) -> GasPriceResult<Option<UpdaterMetadata>> {
        Err(GasPriceError::CouldNotFetchMetadata {
            source_error: anyhow!("boo!"),
        })
    }

    fn set_metadata(&mut self, _: &UpdaterMetadata) -> GasPriceResult<()> {
        Err(GasPriceError::CouldNotSetMetadata {
            block_height: Default::default(),
            source_error: anyhow!("boo!"),
        })
    }
}

struct FakeDABlockCost;

#[async_trait::async_trait]
impl DaBlockCostsSource for FakeDABlockCost {
    async fn request_da_block_cost(&mut self) -> anyhow::Result<DaBlockCosts> {
        todo!()
    }
}

fn arbitrary_config() -> V1AlgorithmConfig {
    V1AlgorithmConfig {
        new_exec_gas_price: 100,
        min_exec_gas_price: 0,
        exec_gas_price_change_percent: 10,
        l2_block_fullness_threshold_percent: 0,
        gas_price_factor: NonZeroU64::new(100).unwrap(),
        min_da_gas_price: 0,
        max_da_gas_price_change_percent: 0,
        da_p_component: 0,
        da_d_component: 0,
        normal_range_size: 0,
        capped_range_size: 0,
        decrease_range_size: 0,
        block_activity_threshold: 0,
        unrecorded_blocks: vec![],
    }
}

fn arbitrary_metadata() -> V1Metadata {
    V1Metadata {
        new_scaled_exec_price: 100,
        l2_block_height: 0,
        new_scaled_da_gas_price: 0,
        gas_price_factor: NonZeroU64::new(100).unwrap(),
        total_da_rewards_excess: 0,
        latest_known_total_da_cost_excess: 0,
        last_profit: 0,
        second_to_last_profit: 0,
        latest_da_cost_per_byte: 0,
        unrecorded_blocks: vec![],
    }
}

fn different_arb_config() -> V1AlgorithmConfig {
    V1AlgorithmConfig {
        new_exec_gas_price: 200,
        min_exec_gas_price: 0,
        exec_gas_price_change_percent: 20,
        l2_block_fullness_threshold_percent: 0,
        gas_price_factor: NonZeroU64::new(100).unwrap(),
        min_da_gas_price: 0,
        max_da_gas_price_change_percent: 0,
        da_p_component: 0,
        da_d_component: 0,
        normal_range_size: 0,
        capped_range_size: 0,
        decrease_range_size: 0,
        block_activity_threshold: 0,
        unrecorded_blocks: vec![],
    }
}

#[tokio::test]
async fn next_gas_price__affected_by_new_l2_block() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    tracing::debug!("test test test test");

    // given
    let l2_block = BlockInfo::Block {
        height: 1,
        gas_used: 60,
        block_gas_capacity: 100,
        block_bytes: 100,
        block_fees: 100,
    };
    let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
    let l2_block_source = FakeL2BlockSource {
        l2_block: l2_block_receiver,
    };
    let metadata_storage = FakeMetadata::empty();

    let config = arbitrary_config();
    let height = 0;
    let (algo_updater, shared_algo) =
        initialize_algorithm(&config, height, &metadata_storage).unwrap();
    let da_source = FakeDABlockCost;
    let da_source_service = DaSourceService::new(da_source, None);
    let mut service = GasPriceServiceV1::new(
        l2_block_source,
        metadata_storage,
        shared_algo,
        algo_updater,
        da_source_service,
    );

    let read_algo = service.next_block_algorithm();
    let initial = read_algo.next_gas_price();
    let mut watcher = StateWatcher::started();

    // when
    service.run(&mut watcher).await.unwrap();
    l2_block_sender.send(l2_block).await.unwrap();
    service.shutdown().await.unwrap();

    // then
    let new = read_algo.next_gas_price();
    assert_ne!(initial, new);
}

// #[tokio::test]
// async fn next__new_l2_block_saves_old_metadata() {
//     // given
//     let l2_block = BlockInfo::Block {
//         height: 1,
//         gas_used: 60,
//         block_gas_capacity: 100,
//         block_bytes: 100,
//         block_fees: 100,
//     };
//     let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
//     let l2_block_source = FakeL2BlockSource {
//         l2_block: l2_block_receiver,
//     };
//     let metadata_inner = Arc::new(std::sync::Mutex::new(None));
//     let metadata_storage = FakeMetadata {
//         inner: metadata_inner.clone(),
//     };
//
//     let config = arbitrary_config();
//     let height = 0;
//     let (algo_updater, shared_algo) =
//         initialize_algorithm(&config, height, &metadata_storage).unwrap();
//
//     let mut service = GasPriceServiceV1::new(
//         l2_block_source,
//         metadata_storage,
//         shared_algo,
//         algo_updater,
//     );
//
//     // when
//     let read_algo = service.next_block_algorithm();
//     let mut watcher = StateWatcher::default();
//     let start = read_algo.next_gas_price();
//
//     service.run(&mut watcher).await.unwrap();
//     l2_block_sender.send(l2_block).await.unwrap();
//     service.shutdown().await.unwrap();
//
//     // then
//     let new = read_algo.next_gas_price();
//     assert_ne!(start, new);
// }

#[derive(Clone)]
struct FakeSettings;

impl GasPriceSettingsProvider for FakeSettings {
    fn settings(
        &self,
        _param_version: &ConsensusParametersVersion,
    ) -> GasPriceResult<GasPriceSettings> {
        unimplemented!()
    }
}

#[derive(Clone)]
struct FakeGasPriceDb;

// GasPriceData + Modifiable + KeyValueInspect<Column = GasPriceColumn>
impl GasPriceData for FakeGasPriceDb {
    fn latest_height(&self) -> Option<BlockHeight> {
        unimplemented!()
    }
}

#[derive(Clone)]
struct FakeOnChainDb {
    height: BlockHeight,
}

impl FakeOnChainDb {
    fn new(height: u32) -> Self {
        Self {
            height: height.into(),
        }
    }
}

struct FakeL2Data {
    height: BlockHeight,
}

impl FakeL2Data {
    fn new(height: BlockHeight) -> Self {
        Self { height }
    }
}

impl L2Data for FakeL2Data {
    fn latest_height(&self) -> StorageResult<BlockHeight> {
        Ok(self.height)
    }

    fn get_block(
        &self,
        _height: &BlockHeight,
    ) -> StorageResult<Option<Block<Transaction>>> {
        unimplemented!()
    }
}
impl AtomicView for FakeOnChainDb {
    type LatestView = FakeL2Data;

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(FakeL2Data::new(self.height))
    }
}

fn empty_block_stream() -> BoxStream<SharedImportResult> {
    let blocks: Vec<Arc<dyn Deref<Target = ImportResult> + Send + Sync>> = vec![];
    tokio_stream::iter(blocks).into_boxed()
}

// #[tokio::test]
// async fn uninitialized_task__new__if_exists_already_reload_old_values_with_overrides() {
//     // given
//     let original_metadata = arbitrary_metadata();
//     let original = UpdaterMetadata::V1(original_metadata.clone());
//     let metadata_inner = Arc::new(std::sync::Mutex::new(Some(original.clone())));
//     let metadata_storage = FakeMetadata {
//         inner: metadata_inner,
//     };
//
//     let different_config = different_arb_config();
//     assert_ne!(
//         different_config.starting_gas_price,
//         original_metadata.new_exec_price
//     );
//     let different_l2_block = 1231;
//     assert_ne!(different_l2_block, original_metadata.l2_block_height);
//     let settings = FakeSettings;
//     let block_stream = empty_block_stream();
//     let gas_price_db = FakeGasPriceDb;
//     let on_chain_db = FakeOnChainDb::new(different_l2_block);
//
//     // when
//     let service = UninitializedTask::new(
//         different_config,
//         0.into(),
//         settings,
//         block_stream,
//         gas_price_db,
//         metadata_storage,
//         on_chain_db,
//     )
//     .unwrap();
//
//     // then
//     let V1Metadata {
//         new_exec_price,
//         l2_block_height,
//     } = original_metadata;
//     let UninitializedTask { algo_updater, .. } = service;
//     assert_eq!(algo_updater.new_exec_price, new_exec_price);
//     assert_eq!(algo_updater.l2_block_height, l2_block_height);
// }

// #[tokio::test]
// async fn uninitialized_task__new__should_fail_if_cannot_fetch_metadata() {
//     // given
//     let config = arbitrary_config();
//     let different_l2_block = 1231;
//     let metadata_storage = ErroringMetadata;
//     let settings = FakeSettings;
//     let block_stream = empty_block_stream();
//     let gas_price_db = FakeGasPriceDb;
//     let on_chain_db = FakeOnChainDb::new(different_l2_block);
//
//     // when
//     let res = UninitializedTask::new(
//         config,
//         0.into(),
//         settings,
//         block_stream,
//         gas_price_db,
//         metadata_storage,
//         on_chain_db,
//     );
//
//     // then
//     let is_err = res.is_err();
//     assert!(is_err);
// }
