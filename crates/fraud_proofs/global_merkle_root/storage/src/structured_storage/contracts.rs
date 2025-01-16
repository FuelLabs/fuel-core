use crate::{
    column::TableColumns,
    merkle::MerklizedTableColumn,
};
use fuel_core_storage::tables::{
    ContractsLatestUtxo,
    ContractsRawCode,
};

impl MerklizedTableColumn for ContractsRawCode {
    fn table_column() -> TableColumns {
        TableColumns::ContractsRawCode
    }
}

impl MerklizedTableColumn for ContractsLatestUtxo {
    fn table_column() -> TableColumns {
        TableColumns::ContractsLatestUtxo
    }
}
