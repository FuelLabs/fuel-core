//! Types related to GraphQL API service.

use crate::fuel_types::{
    Address,
    AssetId,
    ContractId,
};

/// The cumulative balance(`amount`) of the `Owner` of `asset_id`.
pub struct Balance<Owner, Amount> {
    /// Owner of the asset.
    pub owner: Owner,
    /// The cumulative amount of the asset.
    pub amount: Amount,
    /// The identifier of the asset.
    pub asset_id: AssetId,
}

/// The alias for the `Balance` of the address.
pub type AddressBalance = Balance<Address, u128>;

/// The alias for the `Balance` of the contract.
pub type ContractBalance = Balance<ContractId, u64>;
