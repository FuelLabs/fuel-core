use crate::client::{
    schema::{
        self,
        ConversionError,
    },
    types::primitives::MerkleRoot,
};
use fuel_core_types::fuel_vm::UploadedBytecode;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateTransitionBytecode {
    pub root: MerkleRoot,
    pub bytecode: UploadedBytecode,
}

// GraphQL Translation

impl TryFrom<schema::upgrades::StateTransitionBytecode> for StateTransitionBytecode {
    type Error = ConversionError;

    fn try_from(
        value: schema::upgrades::StateTransitionBytecode,
    ) -> Result<Self, Self::Error> {
        let root = value.root.0 .0.as_slice().try_into()?;
        let bytecode = value.bytecode.try_into()?;

        Ok(Self { root, bytecode })
    }
}
