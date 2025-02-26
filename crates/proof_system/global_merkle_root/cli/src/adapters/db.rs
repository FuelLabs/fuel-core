use fuel_core_global_merkle_root_api::ports::GetStateRoot;

/// The state root service database
pub struct StateRootDatabase;

/// The state root service database description
pub struct StateRootDatabaseDescription;

// impl DatabaseDescription for StateRootDatabaseDescription {}

// impl DatabaseDescription for StateRootDatabaseDescription {
//
//    };

impl GetStateRoot for StateRootDatabase {
    fn state_root_at(
        &self,
        height: fuel_core_types::fuel_types::BlockHeight,
    ) -> Result<Option<fuel_core_types::fuel_tx::Bytes32>, fuel_core_storage::Error> {
        todo!()
    }
}
