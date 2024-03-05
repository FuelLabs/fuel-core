use crate::serialization::HexIfHumanReadable;
use fuel_core_types::fuel_types::{
    Bytes32,
    ContractId,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::serde_as;

#[serde_as]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ContractStateConfig {
    pub contract_id: ContractId,
    pub key: Bytes32,
    #[serde_as(as = "HexIfHumanReadable")]
    pub value: Vec<u8>,
}

#[cfg(all(test, feature = "random", feature = "std"))]
impl crate::Randomize for ContractStateConfig {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        Self {
            contract_id: super::random_bytes_32(&mut rng).into(),
            key: super::random_bytes_32(&mut rng).into(),
            value: super::random_bytes_32(&mut rng).into(),
        }
    }
}
