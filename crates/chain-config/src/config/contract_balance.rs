use fuel_core_storage::{
    tables::ContractsAssets,
    ContractsAssetKey,
};
use fuel_core_types::fuel_types::{
    AssetId,
    ContractId,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::MyEntry;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ContractBalanceConfig {
    pub contract_id: ContractId,
    pub asset_id: AssetId,
    pub amount: u64,
}

impl From<MyEntry<ContractsAssets>> for ContractBalanceConfig {
    fn from(entry: MyEntry<ContractsAssets>) -> Self {
        Self {
            contract_id: *entry.key.contract_id(),
            asset_id: *entry.key.asset_id(),
            amount: entry.value,
        }
    }
}

impl From<ContractBalanceConfig> for MyEntry<ContractsAssets> {
    fn from(config: ContractBalanceConfig) -> Self {
        Self {
            key: config.contract_asset_key(),
            value: config.amount,
        }
    }
}

impl ContractBalanceConfig {
    pub fn contract_asset_key(&self) -> ContractsAssetKey {
        (&self.contract_id, &self.asset_id).into()
    }
}

#[cfg(all(test, feature = "random", feature = "std"))]
impl crate::Randomize for ContractBalanceConfig {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        Self {
            contract_id: super::random_bytes_32(&mut rng).into(),
            asset_id: super::random_bytes_32(&mut rng).into(),
            amount: rng.gen(),
        }
    }
}
