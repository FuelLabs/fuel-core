use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::{
    fuel_merkle::common::Bytes32,
    fuel_tx::{
        AssetId,
        ContractId,
    },
};

/// Contract info
pub struct AssetsInfo;

pub type AssetDetails = (ContractId, Bytes32, u64); // (contract_id, sub_id, total_amount)

impl Mappable for AssetsInfo {
    type Key = AssetId;
    type OwnedKey = Self::Key;
    type Value = Self::OwnedValue;
    type OwnedValue = AssetDetails;
}

impl TableWithBlueprint for AssetsInfo {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::AssetsInfo
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fuel_core_storage::basic_storage_tests!(
        AssetsInfo,
        <AssetsInfo as Mappable>::Key::default(),
        <AssetsInfo as Mappable>::Value::default()
    );
}
