use crate::fuel_types::{
    Address,
    AssetId,
};

/// The cumulative balance(`amount`) of the `owner` of `asset_id`.
pub struct Balance {
    /// Owner of the asset.
    pub owner: Address,
    /// The cumulative amount of the asset.
    pub amount: u64,
    /// The identifier of the asset.
    pub asset_id: AssetId,
}
