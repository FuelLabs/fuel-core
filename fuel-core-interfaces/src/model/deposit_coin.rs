use super::{BlockHeight, DaBlockHeight};
use fuel_types::{Address, AssetId, Bytes32, Word};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct DepositCoin {
    pub owner: Address,
    pub amount: Word,
    pub asset_id: AssetId,
    pub nonce: Bytes32,
    pub deposited_da_height: DaBlockHeight,
    pub fuel_block_spend: Option<BlockHeight>,
}

impl DepositCoin {
    /// TODO check what id are we going to use
    /// depends on https://github.com/FuelLabs/fuel-specs/issues/106
    pub fn id(&self) -> Bytes32 {
        self.nonce
    }
}
