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

use crate::graphql_api::ports::OnChainDatabase;

pub trait UpgradeQueryData: Send + Sync {
    fn state_transition_bytecode_root(
        &self,
        version: StateTransitionBytecodeVersion,
    ) -> StorageResult<Bytes32>;

    fn state_transition_bytecode(&self, root: Bytes32)
        -> StorageResult<UploadedBytecode>;
}

impl<D> UpgradeQueryData for D
where
    D: OnChainDatabase + ?Sized,
{
    fn state_transition_bytecode_root(
        &self,
        version: StateTransitionBytecodeVersion,
    ) -> StorageResult<Bytes32> {
        let merkle_root = self
            .storage::<StateTransitionBytecodeVersions>()
            .get(&version)?
            .ok_or(not_found!(StateTransitionBytecodeVersions))?
            .into_owned();

        Ok(merkle_root)
    }

    fn state_transition_bytecode(
        &self,
        root: Bytes32,
    ) -> StorageResult<UploadedBytecode> {
        let bytecode = self
            .storage::<UploadedBytecodes>()
            .get(&root)?
            .ok_or(not_found!(UploadedBytecodes))?
            .into_owned();

        Ok(bytecode)
    }
}
