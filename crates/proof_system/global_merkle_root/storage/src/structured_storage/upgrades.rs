use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
};
use fuel_core_storage::tables::{
    ConsensusParametersVersions,
    StateTransitionBytecodeVersions,
    UploadedBytecodes,
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
