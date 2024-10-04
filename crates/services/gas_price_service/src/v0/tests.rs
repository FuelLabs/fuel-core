#![allow(non_snake_case)]

use crate::{
    common::{
        l2_block_source::L2BlockSource,
        updater_metadata::UpdaterMetadata,
        utils::{
            BlockInfo,
            Error as GasPriceError,
            Result as GasPriceResult,
        },
    },
    ports::MetadataStorage,
    v0::{
        metadata::V0Metadata,
        service::GasPriceServiceV0,
        uninitialized_task::initialize_algorithm,
    },
};
use anyhow::anyhow;
use fuel_core_services::{
    Service,
    ServiceRunner,
};
use fuel_core_types::fuel_types::BlockHeight;
use std::{
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

struct PendingL2BlockSource;

#[async_trait::async_trait]
impl L2BlockSource for PendingL2BlockSource {
    async fn get_l2_block(&mut self) -> GasPriceResult<BlockInfo> {
        futures::future::pending().await
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

fn arb_metadata() -> V0Metadata {
    V0Metadata {
        // set values
        exec_gas_price_change_percent: 10,
        new_exec_price: 100,
        // unset values
        l2_block_height: 0,
        l2_block_fullness_threshold_percent: 0,
        min_exec_gas_price: 0,
    }
}

fn different_arb_metadata() -> V0Metadata {
    V0Metadata {
        // set values
        exec_gas_price_change_percent: 20,
        new_exec_price: 100,
        // unset values
        l2_block_height: 0,
        l2_block_fullness_threshold_percent: 0,
        min_exec_gas_price: 0,
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

    let starting_metadata = arb_metadata();
    let (algo_updater, shared_algo) =
        initialize_algorithm(starting_metadata.clone(), &metadata_storage).unwrap();
    let service = GasPriceServiceV0::new(
        l2_block_source,
        metadata_storage,
        shared_algo,
        algo_updater,
    );
    let service = ServiceRunner::new(service);
    let shared = service.shared.clone();
    let initial = shared.next_gas_price().await;

    // when
    service.start_and_await().await.unwrap();
    l2_block_sender.send(l2_block).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    // then
    let new = shared.next_gas_price().await;
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

    let starting_metadata = arb_metadata();
    let (algo_updater, shared_algo) =
        initialize_algorithm(starting_metadata.clone(), &metadata_storage).unwrap();

    let service = GasPriceServiceV0::new(
        l2_block_source,
        metadata_storage,
        shared_algo,
        algo_updater,
    );

    // when
    let service = ServiceRunner::new(service);
    let shared = service.shared.clone();
    let start = shared.next_gas_price().await;

    service.start_and_await().await.unwrap();
    l2_block_sender.send(l2_block).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    // then
    let new = shared.next_gas_price().await;
    assert_ne!(start, new);
}

#[tokio::test]
async fn new__if_exists_already_reload_old_values_with_overrides() {
    // given
    let original = UpdaterMetadata::V0(arb_metadata());
    let metadata_inner = Arc::new(std::sync::Mutex::new(Some(original.clone())));
    let metadata_storage = FakeMetadata {
        inner: metadata_inner,
    };
    let l2_block_source = PendingL2BlockSource;
    let new_exec_gas_price = 100;
    let new_min_exec_gas_price = 99;
    let new_exec_gas_price_change_percent = 88;
    let new_l2_block_fullness_threshold_percent = 77;
    let new_metadata = V0Metadata {
        exec_gas_price_change_percent: new_exec_gas_price_change_percent,
        new_exec_price: new_exec_gas_price,
        l2_block_fullness_threshold_percent: new_l2_block_fullness_threshold_percent,
        min_exec_gas_price: new_min_exec_gas_price,
        l2_block_height: original.l2_block_height().into(),
    };
    let (algo_updater, shared_algo) =
        initialize_algorithm(new_metadata, &metadata_storage).unwrap();

    // when
    let service = GasPriceServiceV0::new(
        l2_block_source,
        metadata_storage,
        shared_algo,
        algo_updater,
    );

    // then
    let expected = original;
    let actual = service.algorithm_updater().clone().into();
    assert_ne!(expected, actual);
}

#[tokio::test]
async fn initialize_algorithm__should_fail_if_cannot_fetch_metadata() {
    // given
    let metadata_storage = ErroringMetadata;

    // when
    let metadata = different_arb_metadata();
    let res = initialize_algorithm(metadata, &metadata_storage);

    // then
    assert!(matches!(res, Err(GasPriceError::CouldNotInitUpdater(_))));
}
