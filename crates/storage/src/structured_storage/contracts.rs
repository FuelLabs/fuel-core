//! The module contains implementations and tests for the contracts tables.

use crate::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::{
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
    },
};

// # Dev-note: The value of the `ContractsRawCode` has a unique implementation of serialization
// and deserialization and uses `Raw` codec. Because the value is a contract byte code represented
// by bytes, we don't use `serde::Deserialization` and `serde::Serialization` for `Vec`,
// because we don't need to store the size of the contract. We store/load raw bytes.
impl TableWithBlueprint for ContractsRawCode {
    type Blueprint = Plain<Raw, Raw>;
    type Column = Column;

    fn column() -> Column {
        Column::ContractsRawCode
    }
}

impl TableWithBlueprint for ContractsInfo {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::ContractsInfo
    }
}

impl TableWithBlueprint for ContractsLatestUtxo {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::ContractsLatestUtxo
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use fuel_core_types::{
        entities::contract::ContractsInfoType,
        fuel_tx::Salt,
    };

    crate::basic_storage_tests!(
        ContractsRawCode,
        <ContractsRawCode as crate::Mappable>::Key::from([1u8; 32]),
        vec![32u8],
        <ContractsRawCode as crate::Mappable>::OwnedValue::from(vec![32u8])
    );

    crate::basic_storage_tests!(
        ContractsInfo,
        <ContractsInfo as crate::Mappable>::Key::from([1u8; 32]),
        ContractsInfoType::V1(Salt::new([2u8; 32]).into())
    );

    crate::basic_storage_tests!(
        ContractsLatestUtxo,
        <ContractsLatestUtxo as crate::Mappable>::Key::from([1u8; 32]),
        <ContractsLatestUtxo as crate::Mappable>::Value::default()
    );
}
