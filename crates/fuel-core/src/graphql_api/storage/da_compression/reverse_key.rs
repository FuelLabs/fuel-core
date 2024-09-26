use fuel_core_types::{
    fuel_tx::{
        input::PredicateCode,
        ScriptCode,
    },
    fuel_types::{
        Address,
        AssetId,
        Bytes32,
        ContractId,
    },
};
use std::ops::Deref;

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
    Address(Address),
    AssetId(AssetId),
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
            0 => ReverseKey::Address(Address::default()),
            1 => ReverseKey::AssetId(AssetId::default()),
            2 => ReverseKey::ContractId(ContractId::default()),
            3 => ReverseKey::ScriptCode(Bytes32::default()),
            4 => ReverseKey::PredicateCode(Bytes32::default()),
            _ => unreachable!("New reverse key is added but not supported here"),
        }
    }
}
