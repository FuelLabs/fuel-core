use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
};
use fuel_core_storage::tables::{
    ContractsLatestUtxo,
    ContractsRawCode,
};

impl MerkleizedTableColumn for ContractsRawCode {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::ContractsRawCode
    }
}

impl MerkleizedTableColumn for ContractsLatestUtxo {
    type TableColumn = TableColumn;

    fn table_column() -> TableColumn {
        TableColumn::ContractsLatestUtxo
    }
}
