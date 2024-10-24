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
    v0::{
        metadata::{
            V0Metadata,
            V0MetadataInitializer,
        },
        service::GasPriceServiceV0,
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
    Service,
    ServiceRunner,
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

fn arb_config() -> V0MetadataInitializer {
    V0MetadataInitializer {
        starting_gas_price: 100,
        min_gas_price: 0,
        gas_price_change_percent: 10,
        gas_price_threshold_percent: 0,
    }
}

fn arb_metadata() -> V0Metadata {
    V0Metadata {
        new_exec_price: 100,
        l2_block_height: 0,
    }
}

fn different_arb_config() -> V0MetadataInitializer {
    V0MetadataInitializer {
        starting_gas_price: 200,
        min_gas_price: 0,
        gas_price_change_percent: 20,
        gas_price_threshold_percent: 0,
    }
}

#[tokio::test]
async fn next_gas_price__affected_by_new_l2_block() {
    // given
    let l2_block = BlockInfo::Block {
        height: 1,
        gas_used: 60,
        block_gas_capacity: 100,
    };
    let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
    let l2_block_source = FakeL2BlockSource {
        l2_block: l2_block_receiver,
    };
    let metadata_storage = FakeMetadata::empty();

    let config = arb_config();
    let height = 0;
    let (algo_updater, shared_algo) =
        initialize_algorithm(&config, height, &metadata_storage).unwrap();
    let service = GasPriceServiceV0::new(
        l2_block_source,
        metadata_storage,
        shared_algo,
        algo_updater,
    );
    let service = ServiceRunner::new(service);
    let shared = service.shared.clone();
    let initial = shared.next_gas_price();

    // when
    service.start_and_await().await.unwrap();
    l2_block_sender.send(l2_block).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    // then
    let new = shared.next_gas_price();
    assert_ne!(initial, new);
    service.stop_and_await().await.unwrap();
}

#[tokio::test]
async fn next__new_l2_block_saves_old_metadata() {
    // given
    let l2_block = BlockInfo::Block {
        height: 1,
        gas_used: 60,
        block_gas_capacity: 100,
    };
    let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
    let l2_block_source = FakeL2BlockSource {
        l2_block: l2_block_receiver,
    };
    let metadata_inner = Arc::new(std::sync::Mutex::new(None));
    let metadata_storage = FakeMetadata {
        inner: metadata_inner.clone(),
    };

    let config = arb_config();
    let height = 0;
    let (algo_updater, shared_algo) =
        initialize_algorithm(&config, height, &metadata_storage).unwrap();

    let service = GasPriceServiceV0::new(
        l2_block_source,
        metadata_storage,
        shared_algo,
        algo_updater,
    );

    // when
    let service = ServiceRunner::new(service);

    service.start_and_await().await.unwrap();
    l2_block_sender.send(l2_block).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    // then
    assert!(metadata_inner.lock().unwrap().is_some());
}

#[derive(Clone)]
struct FakeSettings;

impl GasPriceSettingsProvider for FakeSettings {
    fn settings(
        &self,
        _param_version: &ConsensusParametersVersion,
    ) -> GasPriceResult<GasPriceSettings> {
        todo!()
    }
}

#[derive(Clone)]
struct FakeGasPriceDb;

// GasPriceData + Modifiable + KeyValueInspect<Column = GasPriceColumn>
impl GasPriceData for FakeGasPriceDb {
    fn latest_height(&self) -> Option<BlockHeight> {
        todo!()
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
        todo!()
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

// TODO: Don't test `initialize_algorithm`. It's not our public api. We want to test the uninitialized task.
#[tokio::test]
async fn initialize_algorithm__if_exists_already_reload_old_values_with_overrides() {
    // given
    let original_metadata = arb_metadata();
    let original = UpdaterMetadata::V0(original_metadata.clone());
    let metadata_inner = Arc::new(std::sync::Mutex::new(Some(original.clone())));
    let metadata_storage = FakeMetadata {
        inner: metadata_inner,
    };

    let different_config = different_arb_config();
    assert_ne!(
        different_config.starting_gas_price,
        original_metadata.new_exec_price
    );
    let different_l2_block = 1231;
    assert_ne!(different_l2_block, original_metadata.l2_block_height);
    let settings = FakeSettings;
    let block_stream = empty_block_stream();
    let gas_price_db = FakeGasPriceDb;
    let on_chain_db = FakeOnChainDb::new(different_l2_block);

    // when
    let service = UninitializedTask::new(
        different_config,
        0.into(),
        settings,
        block_stream,
        gas_price_db,
        metadata_storage,
        on_chain_db,
    )
    .unwrap();

    // then
    let V0Metadata {
        new_exec_price,
        l2_block_height,
    } = original_metadata;
    let UninitializedTask { algo_updater, .. } = service;
    assert_eq!(algo_updater.new_exec_price, new_exec_price);
    assert_eq!(algo_updater.l2_block_height, l2_block_height);
}

// TODO: Don't test `initialize_algorithm`. It's not our public api. We want to test the uninitialized task.
#[tokio::test]
async fn initialize_algorithm__should_fail_if_cannot_fetch_metadata() {
    // given
    let metadata_storage = ErroringMetadata;

    // when
    let config = arb_config();
    let height = 0;
    let res = initialize_algorithm(&config, height, &metadata_storage);

    // then
    assert!(matches!(res, Err(GasPriceError::CouldNotInitUpdater(_))));
}
