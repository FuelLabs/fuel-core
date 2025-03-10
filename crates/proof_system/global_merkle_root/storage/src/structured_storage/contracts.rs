use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    merkle::column::MerkleizedColumn,
    structured_storage::TableWithBlueprint,
};

use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
    tables::{
        ContractsLatestUtxo,
        ContractsRawCode,
    },
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

impl TableWithBlueprint for ContractsRawCode {
    type Blueprint = Plain<Raw, Raw>;
    type Column = MerkleizedColumn<TableColumn>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Self::table_column())
    }
}

impl TableWithBlueprint for ContractsLatestUtxo {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = MerkleizedColumn<TableColumn>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Self::table_column())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fuel_core_storage::basic_storage_tests!(
        ContractsRawCode,
        <ContractsRawCode as fuel_core_storage::Mappable>::Key::from([1u8; 32]),
        vec![32u8],
        <ContractsRawCode as fuel_core_storage::Mappable>::OwnedValue::from(vec![32u8])
    );

    fuel_core_storage::basic_storage_tests!(
        ContractsLatestUtxo,
        <ContractsLatestUtxo as fuel_core_storage::Mappable>::Key::from([1u8; 32]),
        <ContractsLatestUtxo as fuel_core_storage::Mappable>::Value::default()
    );
}
