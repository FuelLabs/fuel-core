use fuel_core::database::{
    database_description::DatabaseDescription,
    Database,
};
use fuel_core_global_merkle_root_api::ports::GetStateRoot;
use fuel_core_global_merkle_root_storage::{
    column::Column,
    merkle::MerkleMetadata,
};
use fuel_core_storage::{
    iter::{
        IterDirection,
        IteratorOverTable as _,
    },
    transactional::{
        Changes,
        Modifiable,
    },
    Result as StorageResult,
};
use fuel_core_types::fuel_types::BlockHeight;
use itertools::Itertools as _;

pub struct StateRootDb(Database<StateRootDbDescription>);

#[derive(Clone, Copy, Debug)]
pub struct StateRootDbDescription;

impl DatabaseDescription for StateRootDbDescription {
    type Column = Column;
    type Height = BlockHeight;

    fn version() -> u32 {
        0
    }

    fn name() -> String {
        String::from("state_root")
    }

    fn metadata_column() -> Self::Column {
        Column::MerkleMetadataColumn
    }

    fn prefix(_column: &Self::Column) -> Option<usize> {
        None
    }
}

impl GetStateRoot for StateRootDb {
    fn state_root_at(
        &self,
        height: fuel_core_types::fuel_types::BlockHeight,
    ) -> Result<Option<fuel_core_types::fuel_tx::Bytes32>, fuel_core_storage::Error> {
        todo!()
    }
}

impl Modifiable for StateRootDb {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        fuel_core::database::commit_changes_with_height_update(
            &mut self.0,
            changes,
            |iter| {
                iter.iter_all_keys::<MerkleMetadata>(Some(IterDirection::Reverse))
                    .map(|res| res.map(BlockHeight::from))
                    .try_collect()
            },
        )
    }
}
