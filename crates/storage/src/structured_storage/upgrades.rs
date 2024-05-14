//! The module contains implementations and tests for the contracts tables.

use crate::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
        raw::Raw,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::{
        ConsensusParametersVersions,
        StateTransitionBytecodeVersions,
    },
};
use fuel_vm_private::storage::UploadedBytecodes;

impl TableWithBlueprint for ConsensusParametersVersions {
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::ConsensusParametersVersions
    }
}

impl TableWithBlueprint for StateTransitionBytecodeVersions {
    type Blueprint = Plain<Primitive<4>, Raw>;
    type Column = Column;

    fn column() -> Column {
        Column::StateTransitionBytecodeVersions
    }
}

impl TableWithBlueprint for UploadedBytecodes {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = Column;

    fn column() -> Self::Column {
        Column::UploadedBytecodes
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use fuel_core_types::fuel_tx::ConsensusParameters;

    fn generate_key(rng: &mut impl rand::Rng) -> u32 {
        rng.next_u32()
    }

    crate::basic_storage_tests!(
        ConsensusParametersVersions,
        <ConsensusParametersVersions as crate::Mappable>::Key::default(),
        ConsensusParameters::default(),
        ConsensusParameters::default(),
        generate_key
    );

    crate::basic_storage_tests!(
        StateTransitionBytecodeVersions,
        0u32,
        <StateTransitionBytecodeVersions as crate::Mappable>::OwnedValue::from([123; 32]),
        <StateTransitionBytecodeVersions as crate::Mappable>::OwnedValue::from([123; 32]),
        generate_key
    );

    crate::basic_storage_tests!(
        UploadedBytecodes,
        <UploadedBytecodes as crate::Mappable>::Key::default(),
        <UploadedBytecodes as crate::Mappable>::OwnedValue::Completed(vec![123; 2048])
    );
}
