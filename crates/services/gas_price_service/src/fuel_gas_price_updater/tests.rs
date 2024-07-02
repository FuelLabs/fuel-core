#![allow(non_snake_case)]

use super::*;
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;

struct FakeL2BlockSource {
    l2_block: Arc<tokio::sync::Mutex<Receiver<BlockInfo>>>,
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
        _block_height: &BlockHeight,
    ) -> Result<Option<UpdaterMetadata>> {
        Ok(self.inner.lock().unwrap().clone())
    }

    fn set_metadata(&mut self, metadata: UpdaterMetadata) -> Result<()> {
        let _ = self.inner.lock().unwrap().replace(metadata);
        Ok(())
    }
}

fn arb_metadata() -> UpdaterMetadata {
    UpdaterMetadata::V0(V0Metadata {
        // set values
        exec_gas_price_change_percent: 10,
        new_exec_price: 100,
        // unset values
        l2_block_height: 0,
        l2_block_fullness_threshold_percent: 0,
        min_exec_gas_price: 0,
    })
}

fn different_arb_metadata() -> UpdaterMetadata {
    UpdaterMetadata::V0(V0Metadata {
        // set values
        exec_gas_price_change_percent: 20,
        new_exec_price: 100,
        // unset values
        l2_block_height: 0,
        l2_block_fullness_threshold_percent: 0,
        min_exec_gas_price: 0,
    })
}

#[tokio::test]
async fn next__fetches_l2_block() {
    // given
    let l2_block = BlockInfo {
        height: 1,
        gas_used: 60,
        block_gas_capacity: 100,
    };
    let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
    let l2_block_source = FakeL2BlockSource {
        l2_block: Arc::new(tokio::sync::Mutex::new(l2_block_receiver)),
    };
    let metadata_storage = FakeMetadata::empty();

    let starting_metadata = arb_metadata();
    let mut updater =
        FuelGasPriceUpdater::init(starting_metadata, l2_block_source, metadata_storage)
            .await
            .unwrap();

    let start = updater.start(0.into());
    // when
    let next = tokio::spawn(async move { updater.next().await });

    l2_block_sender.send(l2_block).await.unwrap();
    let new = next.await.unwrap().unwrap();

    // then
    assert_ne!(start, new);
}

#[tokio::test]
async fn init__if_exists_already_reload() {
    // given
    let metadata = arb_metadata();
    let metadata_inner = Arc::new(std::sync::Mutex::new(Some(metadata.clone())));
    let metadata_storage = FakeMetadata {
        inner: metadata_inner,
    };
    let l2_block_source = PendingL2BlockSource;

    // when
    let different_metadata = different_arb_metadata();
    let updater =
        FuelGasPriceUpdater::init(different_metadata, l2_block_source, metadata_storage)
            .await
            .unwrap();

    // then
    let expected: AlgorithmUpdaterV0 = metadata.try_into().unwrap();
    let actual = updater.inner;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn init__if_it_does_not_exist_create_with_provided_values() {
    // given
    let metadata_storage = FakeMetadata::empty();
    let l2_block_source = PendingL2BlockSource;

    // when
    let metadata = different_arb_metadata();
    let updater =
        FuelGasPriceUpdater::init(metadata.clone(), l2_block_source, metadata_storage)
            .await
            .unwrap();

    // then
    let expected: AlgorithmUpdaterV0 = metadata.try_into().unwrap();
    let actual = updater.inner;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn next__new_l2_block_saves_old_metadata() {
    let _ = tracing_subscriber::fmt::try_init();
    // given
    let l2_block = BlockInfo {
        height: 1,
        gas_used: 60,
        block_gas_capacity: 100,
    };
    let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
    let l2_block_source = FakeL2BlockSource {
        l2_block: Arc::new(tokio::sync::Mutex::new(l2_block_receiver)),
    };
    let metadata_inner = Arc::new(std::sync::Mutex::new(None));
    let metadata_storage = FakeMetadata {
        inner: metadata_inner.clone(),
    };

    let starting_metadata = arb_metadata();
    let mut updater = FuelGasPriceUpdater::init(
        starting_metadata.clone(),
        l2_block_source,
        metadata_storage,
    )
    .await
    .unwrap();

    // when
    let next = tokio::spawn(async move { updater.next().await });
    let actual = metadata_inner.lock().unwrap().clone();
    assert_eq!(None, actual);
    l2_block_sender.send(l2_block.clone()).await.unwrap();
    let _ = next.await.unwrap().unwrap();

    // then
    let expected = starting_metadata;
    let actual = metadata_inner.lock().unwrap().clone().unwrap();
    assert_eq!(expected, actual);
}
