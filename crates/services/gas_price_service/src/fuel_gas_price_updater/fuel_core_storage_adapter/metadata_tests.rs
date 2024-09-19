#![allow(non_snake_case)]

use super::*;
use crate::{
    fuel_gas_price_updater::{
        fuel_core_storage_adapter::storage::{
            GasPriceColumn,
            GasPriceMetadata,
        },
        AlgorithmUpdater,
        UpdaterMetadata,
    },
    ports::GasPriceData,
};
use fuel_core_storage::{
    structured_storage::test::InMemoryStorage,
    transactional::{
        IntoTransaction,
        Modifiable,
        StorageTransaction,
    },
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
};
use fuel_gas_price_algorithm::v0::AlgorithmUpdaterV0;

fn arb_metadata_with_l2_height(l2_height: BlockHeight) -> UpdaterMetadata {
    let inner = AlgorithmUpdaterV0 {
        new_exec_price: 100,
        min_exec_gas_price: 12,
        exec_gas_price_change_percent: 2,
        l2_block_height: l2_height.into(),
        l2_block_fullness_threshold_percent: 0,
    };
    AlgorithmUpdater::V0(inner).into()
}

fn database() -> StorageTransaction<InMemoryStorage<GasPriceColumn>> {
    InMemoryStorage::default().into_transaction()
}

impl GasPriceData for StorageTransaction<InMemoryStorage<GasPriceColumn>> {
    fn get_metadata(
        &self,
        block_height: &BlockHeight,
    ) -> StorageResult<Option<UpdaterMetadata>> {
        self.storage_as_ref::<GasPriceMetadata>()
            .get(block_height)
            .map(|v| v.map(|v| v.as_ref().clone()))
    }

    fn set_metadata(&mut self, metadata: UpdaterMetadata) -> StorageResult<()> {
        self.storage::<GasPriceMetadata>()
            .insert(&metadata.l2_block_height(), &metadata)?;
        self.commit_changes(self.changes().clone())?;
        Ok(())
    }

    fn latest_height(&self) -> Option<BlockHeight> {
        todo!()
    }

    fn rollback_last_block(&self) -> StorageResult<()> {
        todo!()
    }
}

#[tokio::test]
async fn get_metadata__returns_none_if_does_not_exist() {
    // given
    let database = database();
    let block_height: BlockHeight = 1u32.into();

    // when
    let actual = database.get_metadata(&block_height).unwrap();

    // then
    let expected = None;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn set_metadata__can_set_metadata() {
    // given
    let mut database = database();
    let block_height: BlockHeight = 1u32.into();
    let metadata = arb_metadata_with_l2_height(block_height);

    // when
    let actual = database.get_metadata(&block_height).unwrap();
    assert_eq!(None, actual);
    database.set_metadata(metadata.clone()).unwrap();
    let actual = database.get_metadata(&block_height).unwrap();

    // then
    let expected = Some(metadata);
    assert_eq!(expected, actual);
}
