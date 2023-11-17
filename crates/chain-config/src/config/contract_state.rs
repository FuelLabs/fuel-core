use fuel_core_types::fuel_types::Bytes32;
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::{
    serde_as,
    skip_serializing_none,
};

#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ContractState {
    pub key: Bytes32,
    pub value: Bytes32,
}

#[cfg(all(test, feature = "random"))]
impl ContractState {
    pub fn random(rng: &mut impl ::rand::Rng) -> Self {
        Self {
            key: super::random_bytes_32(rng).into(),
            value: super::random_bytes_32(rng).into(),
        }
    }
}
