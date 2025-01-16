use crate::{
    column::TableColumn,
    merkle::MerklizedTableColumn,
};
use fuel_core_storage::tables::{
    ConsensusParametersVersions,
    StateTransitionBytecodeVersions,
    UploadedBytecodes,
};

impl MerklizedTableColumn for ConsensusParametersVersions {
    fn table_column() -> TableColumn {
        TableColumn::ConsensusParametersVersions
    }
}

impl MerklizedTableColumn for StateTransitionBytecodeVersions {
    fn table_column() -> TableColumn {
        TableColumn::StateTransitionBytecodeVersions
    }
}

impl MerklizedTableColumn for UploadedBytecodes {
    fn table_column() -> TableColumn {
        TableColumn::UploadedBytecodes
    }
}
