//! Types related to block producer service.

use crate::{
    blockchain::header::{
        ConsensusParametersVersion,
        PartialBlockHeader,
    },
    fuel_tx::{
        AssetId,
        ContractId,
    },
};
use alloc::collections::BTreeMap;

/// Producer instructions for a non-base fee asset.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FeeAssetGasPrice {
    /// Gas price in atomic units of the fee asset.
    pub gas_price: u64,
    /// Emit a Mint even when no transaction pays in this asset.
    pub announce: bool,
}

/// The components required to produce a block.
#[derive(Debug)]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Components<Source> {
    /// The partial block header of the future block without transactions related information.
    pub header_to_produce: PartialBlockHeader,
    /// The source of transactions potentially includable into the future block.
    /// It can be a predefined vector of transactions, a stream of transactions,
    /// or any other type that carries the transactions.
    pub transactions_source: Source,
    /// The `ContractId` of the fee recipient.
    pub coinbase_recipient: ContractId,
    /// The gas price for the base asset.
    pub gas_price: u64,
    /// Prices in atomic units of each accepted non-base fee asset.
    /// WASM transports these separately to preserve the V1 input layout.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub fee_asset_gas_prices: BTreeMap<AssetId, FeeAssetGasPrice>,
}

impl<T> Components<T> {
    /// Getter for the `ConsensusParametersVersion` of the future block.
    pub fn consensus_parameters_version(&self) -> ConsensusParametersVersion {
        self.header_to_produce
            .application
            .consensus_parameters_version
    }
}
