#![allow(non_snake_case)]

use super::*;
use std::sync::Arc;
use tokio::sync::{
    mpsc::Receiver,
    Mutex,
};

struct FakeL2BlockSource {
    l2_block: Arc<Mutex<Receiver<BlockInfo>>>,
}

#[async_trait::async_trait]
impl L2BlockSource for FakeL2BlockSource {
    async fn get_l2_block(&mut self, _height: BlockHeight) -> Result<BlockInfo> {
        let block = self.l2_block.lock().await.recv().await.unwrap();
        Ok(block)
    }
}

struct PendingL2BlockSource;

#[async_trait::async_trait]
impl L2BlockSource for PendingL2BlockSource {
    async fn get_l2_block(&mut self, _height: BlockHeight) -> Result<BlockInfo> {
        futures::future::pending().await
    }
}

struct FakeMetadata {
    inner: Arc<Mutex<Option<UpdaterMetadata>>>,
}

impl FakeMetadata {
    fn empty() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait::async_trait]
impl MetadataStorage for FakeMetadata {
    async fn get_metadata(
        &self,
        _block_height: &BlockHeight,
    ) -> Result<Option<UpdaterMetadata>> {
        Ok(self.inner.lock().await.clone())
    }

    async fn set_metadata(&mut self, metadata: UpdaterMetadata) -> Result<()> {
        let _ = self.inner.lock().await.replace(metadata);
        Ok(())
    }
}

fn arb_inner_updater() -> AlgorithmUpdaterV1 {
    AlgorithmUpdaterV1 {
        // set values
        exec_gas_price_change_percent: 10,
        new_exec_price: 100,
        // unset values
        l2_block_height: 0,
        l2_block_fullness_threshold_percent: 0,
        min_da_gas_price: 0,
        max_da_gas_price_change_percent: 0,
        total_da_rewards: 0,
        da_recorded_block_height: 0,
        latest_known_total_da_cost: 0,
        projected_total_da_cost: 0,
        da_p_component: 0,
        da_d_component: 0,
        profit_avg: 0,
        avg_window: 0,
        latest_da_cost_per_byte: 0,
        last_da_gas_price: 0,
        unrecorded_blocks: vec![],
        min_exec_gas_price: 0,
    }
}

fn different_inner_updater() -> AlgorithmUpdaterV1 {
    AlgorithmUpdaterV1 {
        // set values
        exec_gas_price_change_percent: 20,
        new_exec_price: 100,
        // unset values
        l2_block_height: 0,
        l2_block_fullness_threshold_percent: 0,
        min_da_gas_price: 0,
        max_da_gas_price_change_percent: 0,
        total_da_rewards: 0,
        da_recorded_block_height: 0,
        latest_known_total_da_cost: 0,
        projected_total_da_cost: 0,
        da_p_component: 0,
        da_d_component: 0,
        profit_avg: 0,
        avg_window: 0,
        latest_da_cost_per_byte: 0,
        last_da_gas_price: 0,
        unrecorded_blocks: vec![],
        min_exec_gas_price: 0,
    }
}

#[tokio::test]
async fn next__fetches_l2_block() {
    // given
    let l2_block = BlockInfo {
        height: 1,
        fullness: (60, 100),
        block_bytes: 1000,
        gas_price: 200,
    };
    let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
    let l2_block_source = FakeL2BlockSource {
        l2_block: Arc::new(Mutex::new(l2_block_receiver)),
    };
    let metadata_storage = FakeMetadata::empty();

    let inner = arb_inner_updater();
    let mut updater =
        FuelGasPriceUpdater::init(inner.into(), l2_block_source, metadata_storage)
            .await
            .unwrap();

    let start = updater.start(0.into());
    // when
    let next = tokio::spawn(async move { updater.next().await });

    l2_block_sender.send(l2_block).await.unwrap();
    let new = next.await.unwrap().unwrap();

    // then
    dbg!(&start, &new);
    assert_ne!(start, new);
}

#[tokio::test]
async fn init__if_exists_already_reload() {
    // given
    let metadata = arb_inner_updater();
    let metadata_inner = Arc::new(Mutex::new(Some(metadata.into())));
    let metadata_storage = FakeMetadata {
        inner: metadata_inner,
    };
    let l2_block_source = PendingL2BlockSource;

    // when
    let different_metadata = different_inner_updater();
    let updater = FuelGasPriceUpdater::init(
        different_metadata.into(),
        l2_block_source,
        metadata_storage,
    )
    .await
    .unwrap();

    // then
    let expected = arb_inner_updater();
    let actual = updater.inner;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn init__if_it_does_not_exist_create_with_provided_values() {
    // given
    let metadata_storage = FakeMetadata::empty();
    let l2_block_source = PendingL2BlockSource;

    // when
    let metadata = different_inner_updater();
    let updater =
        FuelGasPriceUpdater::init(metadata.into(), l2_block_source, metadata_storage)
            .await
            .unwrap();

    // then
    let expected = different_inner_updater();
    let actual = updater.inner;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn next__new_l2_block_updates_metadata() {
    // given
    let l2_block = BlockInfo {
        height: 1,
        fullness: (60, 100),
        block_bytes: 1000,
        gas_price: 200,
    };
    let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
    let l2_block_source = FakeL2BlockSource {
        l2_block: Arc::new(Mutex::new(l2_block_receiver)),
    };
    let metadata_inner = Arc::new(Mutex::new(None));
    let metadata_storage = FakeMetadata {
        inner: metadata_inner.clone(),
    };

    let mut inner = arb_inner_updater();
    let mut updater = FuelGasPriceUpdater::init(
        inner.clone().into(),
        l2_block_source,
        metadata_storage,
    )
    .await
    .unwrap();

    // when
    let next = tokio::spawn(async move { updater.next().await });

    l2_block_sender.send(l2_block.clone()).await.unwrap();
    let _ = next.await.unwrap().unwrap();

    // then
    inner
        .update_l2_block_data(
            l2_block.height,
            l2_block.fullness,
            l2_block.block_bytes,
            l2_block.gas_price,
        )
        .unwrap();
    let expected = inner;
    let actual = metadata_inner.lock().await.clone().unwrap().into();
    assert_eq!(expected, actual);
}
