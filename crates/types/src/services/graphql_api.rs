//! Types related to GraphQL API service.

use crate::fuel_types::{
    Address,
    AssetId,
    ContractId,
};

/// The cumulative balance(`amount`) of the `Owner` of `asset_id`.
pub struct Balance<Owner> {
    /// Owner of the asset.
    pub owner: Owner,
    /// The cumulative amount of the asset.
    pub amount: u128,
    /// The identifier of the asset.
    pub asset_id: AssetId,
}

/// The alias for the `Balance` of the address.
pub type AddressBalance = Balance<Address>;

/// The alias for the `Balance` of the contract.
pub type ContractBalance = Balance<ContractId>;
