use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
        raw::Raw,
    },
    merkle::column::MerkleizedColumn,
    structured_storage::TableWithBlueprint,
};

use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
    tables::{
        ConsensusParametersVersions,
        StateTransitionBytecodeVersions,
        UploadedBytecodes,
    },
};

impl MerkleizedTableColumn for ConsensusParametersVersions {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::ConsensusParametersVersions
    }
}

impl MerkleizedTableColumn for StateTransitionBytecodeVersions {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::StateTransitionBytecodeVersions
    }
}

impl MerkleizedTableColumn for UploadedBytecodes {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::UploadedBytecodes
    }
}

impl TableWithBlueprint for ConsensusParametersVersions {
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = MerkleizedColumn<TableColumn>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Self::table_column())
    }
}

impl TableWithBlueprint for StateTransitionBytecodeVersions {
    type Blueprint = Plain<Primitive<4>, Raw>;
    type Column = MerkleizedColumn<TableColumn>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Self::table_column())
    }
}

impl TableWithBlueprint for UploadedBytecodes {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = MerkleizedColumn<TableColumn>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Self::table_column())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use fuel_core_types::fuel_tx::ConsensusParameters;

    fn generate_key(rng: &mut impl rand::Rng) -> u32 {
        rng.next_u32()
    }

    fuel_core_storage::basic_storage_tests!(
        ConsensusParametersVersions,
        <ConsensusParametersVersions as fuel_core_storage::Mappable>::Key::default(),
        ConsensusParameters::default(),
        ConsensusParameters::default(),
        generate_key
    );

    fuel_core_storage::basic_storage_tests!(
        StateTransitionBytecodeVersions,
        0u32,
        <StateTransitionBytecodeVersions as fuel_core_storage::Mappable>::OwnedValue::from([123; 32]),
        <StateTransitionBytecodeVersions as fuel_core_storage::Mappable>::OwnedValue::from([123; 32]),
        generate_key
    );

    fuel_core_storage::basic_storage_tests!(
        UploadedBytecodes,
        <UploadedBytecodes as fuel_core_storage::Mappable>::Key::default(),
        <UploadedBytecodes as fuel_core_storage::Mappable>::OwnedValue::Completed(
            vec![123; 2048]
        )
    );
}
