use fuel_core_storage::ContractsAssetKey;
use fuel_core_types::fuel_types::{
    AssetId,
    ContractId,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ContractBalanceConfig {
    pub contract_id: ContractId,
    pub asset_id: AssetId,
    pub amount: u64,
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
