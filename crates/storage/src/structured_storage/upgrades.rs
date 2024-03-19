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
        <StateTransitionBytecodeVersions as crate::Mappable>::Key::default(),
        vec![32u8],
        <StateTransitionBytecodeVersions as crate::Mappable>::OwnedValue::from(vec![
            32u8
        ]),
        generate_key
    );
}
