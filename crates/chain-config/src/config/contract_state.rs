use fuel_core_types::fuel_types::{
    Bytes32,
    ContractId,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::serialization::NonSkippingSerialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
// If any fields are added make sure to update the `NonSkippingSerialize` impl
pub struct ContractStateConfig {
    pub contract_id: ContractId,
    pub key: Bytes32,
    pub value: Bytes32,
}

impl NonSkippingSerialize for ContractStateConfig {}

#[cfg(all(test, feature = "random"))]
impl crate::Randomize for ContractStateConfig {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        Self {
            contract_id: super::random_bytes_32(&mut rng).into(),
            key: super::random_bytes_32(&mut rng).into(),
            value: super::random_bytes_32(&mut rng).into(),
        }
    }
}
