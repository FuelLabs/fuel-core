use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    storage_interlayer::Interlayer,
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_txpool::types::ContractId;
use fuel_core_types::entities::contract::ContractsInfoType;

/// Contract info
pub struct ContractsInfo;

impl Mappable for ContractsInfo {
    type Key = Self::OwnedKey;
    type OwnedKey = ContractId;
    type Value = Self::OwnedValue;
    type OwnedValue = ContractsInfoType;
}

impl TableWithBlueprint for ContractsInfo {
    type Blueprint = Plain;
}

impl Interlayer for ContractsInfo {
    type KeyCodec = Raw;
    type ValueCodec = Postcard;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::ContractsInfo
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use fuel_core_types::fuel_tx::Salt;

    fuel_core_storage::basic_storage_tests!(
        ContractsInfo,
        <ContractsInfo as Mappable>::Key::from([1u8; 32]),
        ContractsInfoType::V1(Salt::new([2u8; 32]).into())
    );
}
