use crate::database::{
    database_description::DatabaseDescription,
    Database,
};
use fuel_core_global_merkle_root_api::ports::GetStateRoot;
use fuel_core_global_merkle_root_storage::column::Column;
use fuel_core_storage::transactional::{
    Changes,
    Modifiable,
};
use fuel_core_types::fuel_types::BlockHeight;

#[derive(Clone, Copy, Debug)]
pub struct StateRootDatabase;

impl DatabaseDescription for StateRootDatabase {
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

impl Modifiable for Database<StateRootDatabase> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        crate::database::commit_changes_with_height_update(self, changes, |iter| {
            iter.iter_all_keys::<S>(Some(IterDirection::Reverse))
                .try_collect()
        })
    }
}
