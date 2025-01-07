use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::fuel_tx::{
    AssetId,
    Bytes32,
    ContractId,
};

/// Asset info table to store information about the asset like total minted amounts,
/// source contract, original sub id, etc.
pub struct AssetsInfo;

#[derive(Default, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetDetails {
    pub contract_id: ContractId,
    pub sub_id: Bytes32,
    pub total_supply: u128,
}

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
