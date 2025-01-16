use crate::{
    column::TableColumns,
    merkle::MerklizedTableColumn,
};
use fuel_core_storage::tables::{
    ConsensusParametersVersions,
    StateTransitionBytecodeVersions,
    UploadedBytecodes,
};

impl MerklizedTableColumn for ConsensusParametersVersions {
    fn table_column() -> TableColumns {
        TableColumns::ConsensusParametersVersions
    }
}

impl MerklizedTableColumn for StateTransitionBytecodeVersions {
    fn table_column() -> TableColumns {
        TableColumns::StateTransitionBytecodeVersions
    }
}

impl MerklizedTableColumn for UploadedBytecodes {
    fn table_column() -> TableColumns {
        TableColumns::UploadedBytecodes
    }
}
