use fuel_core::database::database_description::DatabaseDescription;
use fuel_core_global_merkle_root_api::ports::GetStateRoot;
use fuel_core_global_merkle_root_storage::column::Column;
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

impl GetStateRoot for StateRootDatabase {
    fn state_root_at(
        &self,
        height: fuel_core_types::fuel_types::BlockHeight,
    ) -> Result<Option<fuel_core_types::fuel_tx::Bytes32>, fuel_core_storage::Error> {
        todo!()
    }
}
