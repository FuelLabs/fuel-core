use crate::serialization::{
    HexNumber,
    HexType,
};
use fuel_core_types::{
    fuel_tx::{
        Contract,
        ContractId,
        StorageSlot,
    },
    fuel_types::{
        AssetId,
        Bytes32,
        Salt,
    },
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::{
    serde_as,
    skip_serializing_none,
};

#[skip_serializing_none]
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ContractConfig {
    #[serde_as(as = "HexType")]
    pub code: Vec<u8>,
    #[serde_as(as = "HexType")]
    pub salt: Salt,
    #[serde_as(as = "Option<Vec<(HexType, HexType)>>")]
    #[serde(default)]
    pub state: Option<Vec<(Bytes32, Bytes32)>>,
    #[serde_as(as = "Option<Vec<(HexType, HexNumber)>>")]
    #[serde(default)]
    pub balances: Option<Vec<(AssetId, u64)>>,
}

impl ContractConfig {
    pub fn unpack(self) -> (ContractId, Vec<u8>, Salt, Bytes32, Vec<StorageSlot>) {
        let bytes = self.code;
        let salt = self.salt;
        let slots = self.state.map(|slots| {
            slots
                .into_iter()
                .map(|(key, value)| StorageSlot::new(key, value))
                .collect::<Vec<_>>()
        });
        let state_root = slots
            .as_ref()
            .map(|slots| Contract::initial_state_root(slots.iter()))
            .unwrap_or(Contract::default_state_root());
        let contract = Contract::from(bytes.clone());
        let root = contract.root();
        let contract_id = contract.id(&salt, &root, &state_root);
        (
            contract_id,
            bytes,
            salt,
            state_root,
            slots.unwrap_or_default(),
        )
    }
}
