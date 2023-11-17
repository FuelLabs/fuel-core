use fuel_core_types::fuel_types::AssetId;
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
pub struct ContractBalance {
    pub asset_id: AssetId,
    pub amount: u64,
}

#[cfg(all(test, feature = "random"))]
impl ContractBalance {
    pub fn random(rng: &mut impl ::rand::Rng) -> Self {
        Self {
            asset_id: super::random_bytes_32(rng).into(),
            amount: rng.gen(),
        }
    }
}
