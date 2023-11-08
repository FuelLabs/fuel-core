use fuel_core_types::fuel_types::AssetId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContractBalance {
    asset_id: AssetId,
    amount: u64,
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
