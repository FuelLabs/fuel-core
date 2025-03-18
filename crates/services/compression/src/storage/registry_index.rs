//! Registry index table

use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    merkle::sparse::MerkleizedTableColumn,
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::{
    fuel_compression::RegistryKey,
    fuel_tx::{
        input::PredicateCode,
        Address,
        AssetId,
        Bytes32,
        ContractId,
        ScriptCode,
    },
};
use std::ops::Deref;

use super::column::{
    CompressionColumn,
    MerkleizedColumnOf,
};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    strum::EnumCount,
)]
/// The reverse key for the temporal registry index.
/// By this key we can find the registry key from the temporal registry.
pub enum ReverseKey {
    /// Address
    Address(Address),
    /// Asset ID
    AssetId(AssetId),
    /// Contract ID
    ContractId(ContractId),
    /// Hash of the script code.
    ScriptCode(Bytes32),
    /// Hash of the predicate code.
    PredicateCode(Bytes32),
}

impl From<&Address> for ReverseKey {
    fn from(address: &Address) -> Self {
        Self::Address(*address)
    }
}

impl From<&AssetId> for ReverseKey {
    fn from(asset_id: &AssetId) -> Self {
        Self::AssetId(*asset_id)
    }
}

impl From<&ContractId> for ReverseKey {
    fn from(contract_id: &ContractId) -> Self {
        Self::ContractId(*contract_id)
    }
}

impl From<&ScriptCode> for ReverseKey {
    fn from(script_code: &ScriptCode) -> Self {
        let hash = fuel_core_types::fuel_crypto::Hasher::hash(script_code.deref());
        ReverseKey::ScriptCode(hash)
    }
}

impl From<&PredicateCode> for ReverseKey {
    fn from(predicate_code: &PredicateCode) -> Self {
        let hash = fuel_core_types::fuel_crypto::Hasher::hash(predicate_code.deref());
        ReverseKey::PredicateCode(hash)
    }
}

#[cfg(feature = "test-helpers")]
impl rand::distributions::Distribution<ReverseKey> for rand::distributions::Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> ReverseKey {
        use strum::EnumCount;
        match rng.next_u32() as usize % ReverseKey::COUNT {
            0 => ReverseKey::Address(rng.gen()),
            1 => ReverseKey::AssetId(rng.gen()),
            2 => ReverseKey::ContractId(rng.gen()),
            3 => ReverseKey::ScriptCode(rng.gen()),
            4 => ReverseKey::PredicateCode(rng.gen()),
            _ => unreachable!("New reverse key is added but not supported here"),
        }
    }
}

/// Table that indexes the script codes
pub struct RegistryIndex;

impl MerkleizedTableColumn for RegistryIndex {
    type TableColumn = CompressionColumn;

    fn table_column() -> Self::TableColumn {
        Self::TableColumn::RegistryIndex
    }
}

impl Mappable for RegistryIndex {
    type Key = Self::OwnedKey;
    type OwnedKey = ReverseKey;
    type Value = Self::OwnedValue;
    type OwnedValue = RegistryKey;
}

impl TableWithBlueprint for RegistryIndex {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = MerkleizedColumnOf<Self>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Self::table_column())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::fuel_tx::Address;

    fuel_core_storage::basic_storage_tests!(
        RegistryIndex,
        ReverseKey::Address(Address::zeroed()),
        RegistryKey::ZERO
    );
}
