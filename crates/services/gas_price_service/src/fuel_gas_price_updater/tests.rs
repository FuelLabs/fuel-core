#![allow(non_snake_case)]

use super::*;
use std::sync::Arc;

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
    let l2_block = BlockInfo::Block {
        height: 1,
        gas_used: 60,
        block_gas_capacity: 100,
    };
    let metadata_storage = FakeMetadata::empty();

    let starting_metadata = arb_metadata();
    let mut updater =
        FuelGasPriceUpdater::new(starting_metadata.into(), metadata_storage);

    let start = updater.start(0.into());
    // when
    let next = tokio::spawn(async move { updater.next(l2_block, None).await });

    let new = next.await.unwrap().unwrap();

    // then
    assert_ne!(start, new);
}

#[tokio::test]
async fn next__new_l2_block_saves_old_metadata() {
    // given
    let l2_block = BlockInfo::Block {
        height: 1,
        gas_used: 60,
        block_gas_capacity: 100,
    };
    let metadata_inner = Arc::new(std::sync::Mutex::new(None));
    let metadata_storage = FakeMetadata {
        inner: metadata_inner.clone(),
    };

    let starting_metadata = arb_metadata();
    let mut updater =
        FuelGasPriceUpdater::new(starting_metadata.into(), metadata_storage);

    let start = updater.start(0.into());
    // when
    let next = tokio::spawn(async move { updater.next(l2_block, None).await });

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
    let new_min_exec_gas_price = 99;
    let new_exec_gas_price_change_percent = 88;
    let new_l2_block_fullness_threshold_percent = 77;

    // when
    let height = original.l2_block_height();
    let updater = FuelGasPriceUpdater::init(
        height,
        metadata_storage,
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

    // when
    let metadata = different_arb_metadata();
    let height = u32::from(metadata.l2_block_height()) + 1;
    let res = FuelGasPriceUpdater::init(height.into(), metadata_storage, 0, 0, 0);

    // then
    assert!(matches!(res, Err(Error::CouldNotInitUpdater(_))));
}
