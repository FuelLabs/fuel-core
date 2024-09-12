#![allow(non_snake_case)]

use super::*;
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;

struct FakeL2BlockSource {
    l2_block: Receiver<BlockInfo>,
}

#[async_trait::async_trait]
impl L2BlockSource for FakeL2BlockSource {
    async fn get_l2_block(&mut self, _height: BlockHeight) -> Result<BlockInfo> {
        let block = self.l2_block.recv().await.unwrap();
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

#[derive(Default, Clone)]
struct FakeDaSource {
    called: Arc<std::sync::Mutex<bool>>,
}

impl FakeDaSource {
    fn new() -> Self {
        Self {
            called: Arc::new(std::sync::Mutex::new(false)),
        }
    }

    fn was_called(&self) -> bool {
        *self.called.lock().unwrap()
    }
}

impl GetDaBlockCosts for FakeDaSource {
    fn get(&self) -> Result<Option<DaBlockCosts>> {
        *self.called.lock().unwrap() = true;
        Ok(Some(DaBlockCosts::default()))
    }
}

#[tokio::test]
async fn next__fetches_l2_block() {
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
    let fake_da_source = FakeDaSource::new();
    let mut updater = FuelGasPriceUpdater::new(
        starting_metadata.into(),
        l2_block_source,
        metadata_storage,
        fake_da_source.clone(),
    );

    let start = updater.start(0.into());
    // when
    let next = tokio::spawn(async move { updater.next().await });

    l2_block_sender.send(l2_block).await.unwrap();
    let new = next.await.unwrap().unwrap();

    // then
    assert_ne!(start, new);
    match start {
        Algorithm::V0(_) => {}
        Algorithm::V1(_) => {
            assert!(fake_da_source.was_called());
        }
    }
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
    let mut updater = FuelGasPriceUpdater::new(
        starting_metadata.into(),
        l2_block_source,
        metadata_storage,
        FakeDaSource::default(),
    );

    let start = updater.start(0.into());
    // when
    let next = tokio::spawn(async move { updater.next().await });

    l2_block_sender.send(l2_block).await.unwrap();
    let new = next.await.unwrap().unwrap();

    // then
    assert_ne!(start, new);
}

#[tokio::test]
async fn init__if_exists_already_reload_old_values_with_overrides() {
    // given
    let original = arb_metadata();
    let metadata_inner = Arc::new(std::sync::Mutex::new(Some(original.clone())));
    let metadata_storage = FakeMetadata {
        inner: metadata_inner,
    };
    let l2_block_source = PendingL2BlockSource;
    let new_min_exec_gas_price = 99;
    let new_exec_gas_price_change_percent = 88;
    let new_l2_block_fullness_threshold_percent = 77;

    // when
    let height = original.l2_block_height();
    let updater = FuelGasPriceUpdater::init(
        height,
        l2_block_source,
        metadata_storage,
        FakeDaSource::default(),
        new_min_exec_gas_price,
        new_exec_gas_price_change_percent,
        new_l2_block_fullness_threshold_percent,
    )
    .unwrap();

    // then
    let UpdaterMetadata::V0(original_inner) = original;
    let expected: AlgorithmUpdater = UpdaterMetadata::V0(V0Metadata {
        min_exec_gas_price: new_min_exec_gas_price,
        exec_gas_price_change_percent: new_exec_gas_price_change_percent,
        l2_block_fullness_threshold_percent: new_l2_block_fullness_threshold_percent,
        ..original_inner
    })
    .into();
    let actual = updater.inner;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn init__if_it_does_not_exist_fail() {
    // given
    let metadata_storage = FakeMetadata::empty();
    let l2_block_source = PendingL2BlockSource;

    // when
    let metadata = different_arb_metadata();
    let height = u32::from(metadata.l2_block_height()) + 1;
    let res = FuelGasPriceUpdater::init(
        height.into(),
        l2_block_source,
        metadata_storage,
        FakeDaSource::default(),
        0,
        0,
        0,
    );

    // then
    assert!(matches!(res, Err(Error::CouldNotInitUpdater(_))));
}
