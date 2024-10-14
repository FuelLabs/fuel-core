use crate::fuel_core_graphql_api::database::ReadView;
use fuel_core_storage::{
    not_found,
    tables::{
        StateTransitionBytecodeVersions,
        UploadedBytecodes,
    },
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::header::StateTransitionBytecodeVersion,
    fuel_tx::Bytes32,
    fuel_vm::UploadedBytecode,
};

impl ReadView {
    pub fn state_transition_bytecode_root(
        &self,
        version: StateTransitionBytecodeVersion,
    ) -> StorageResult<Bytes32> {
        let merkle_root = self
            .on_chain
            .as_ref()
            .storage::<StateTransitionBytecodeVersions>()
            .get(&version)?
            .ok_or(not_found!(StateTransitionBytecodeVersions))?
            .into_owned();

        Ok(merkle_root)
    }

    pub fn state_transition_bytecode(
        &self,
        root: Bytes32,
    ) -> StorageResult<UploadedBytecode> {
        let bytecode = self
            .on_chain
            .as_ref()
            .storage::<UploadedBytecodes>()
            .get(&root)?
            .ok_or(not_found!(UploadedBytecodes))?
            .into_owned();

        Ok(bytecode)
    }
}
