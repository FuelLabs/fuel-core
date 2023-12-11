use fuel_core_types::fuel_types::Bytes32;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractStateConfig {
    pub key: Bytes32,
    pub value: Bytes32,
}

#[cfg(all(test, feature = "random"))]
impl ContractStateConfig {
    pub fn random(rng: &mut impl ::rand::Rng) -> Self {
        Self {
            key: super::random_bytes_32(rng).into(),
            value: super::random_bytes_32(rng).into(),
        }
    }
}
