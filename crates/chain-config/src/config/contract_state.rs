use crate::{
    serialization::HexIfHumanReadable,
    MyEntry,
};
use fuel_core_storage::{
    tables::ContractsState,
    ContractsStateKey,
};
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

impl From<MyEntry<ContractsState>> for ContractStateConfig {
    fn from(value: MyEntry<ContractsState>) -> Self {
        Self {
            contract_id: *value.key.contract_id(),
            key: *value.key.state_key(),
            value: value.value.into(),
        }
    }
}

impl From<ContractStateConfig> for MyEntry<ContractsState> {
    fn from(value: ContractStateConfig) -> Self {
        MyEntry {
            key: ContractsStateKey::new(&value.contract_id, &value.key),
            value: value.value.into(),
        }
    }
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
