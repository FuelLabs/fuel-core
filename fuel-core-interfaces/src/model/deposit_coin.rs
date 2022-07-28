use super::{BlockHeight, DaBlockHeight};
use fuel_types::{Address, AssetId, Bytes32, Word};

/// Probably going to be superseded with bridge message https://github.com/FuelLabs/fuel-core/issues/366
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
    pub fn id(&self) -> Bytes32 {
        self.nonce
    }
}
