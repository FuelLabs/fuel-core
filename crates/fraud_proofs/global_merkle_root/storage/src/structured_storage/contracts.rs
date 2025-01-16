use crate::{
    column::TableColumn,
    merkle::MerklizedTableColumn,
};
use fuel_core_storage::tables::{
    ContractsLatestUtxo,
    ContractsRawCode,
};

impl MerklizedTableColumn for ContractsRawCode {
    fn table_column() -> TableColumn {
        TableColumn::ContractsRawCode
    }
}

impl MerklizedTableColumn for ContractsLatestUtxo {
    fn table_column() -> TableColumn {
        TableColumn::ContractsLatestUtxo
    }
}
