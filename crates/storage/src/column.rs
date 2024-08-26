//! The module defines the `Column` and default tables used by the current `fuel-core` codebase.
//! In the future, the `Column` enum should contain only the required tables for the execution.
//! All other tables should live in the downstream creates in the place where they are really used.

use crate::kv_store::StorageColumn;

#[cfg(feature = "alloc")]
use alloc::string::{
    String,
    ToString,
};

/// Database tables column ids to the corresponding [`crate::Mappable`] table.
#[repr(u32)]
#[derive(
    Copy,
    Clone,
    Debug,
    strum_macros::EnumCount,
    strum_macros::IntoStaticStr,
    PartialEq,
    Eq,
    enum_iterator::Sequence,
    Hash,
    num_enum::TryFromPrimitive,
)]
pub enum Column {
    /// The column id of metadata about the blockchain
    Metadata = 0,
    /// See [`ContractsRawCode`](crate::tables::ContractsRawCode)
    ContractsRawCode = 1,
    /// See [`ContractsState`](crate::tables::ContractsState)
    ContractsState = 2,
    /// See [`ContractsLatestUtxo`](crate::tables::ContractsLatestUtxo)
    ContractsLatestUtxo = 3,
    /// See [`ContractsAssets`](crate::tables::ContractsAssets)
    ContractsAssets = 4,
    /// See [`Coins`](crate::tables::Coins)
    Coins = 5,
    /// See [`Transactions`](crate::tables::Transactions)
    Transactions = 6,
    /// See [`FuelBlocks`](crate::tables::FuelBlocks)
    FuelBlocks = 7,
    /// See [`FuelBlockMerkleData`](crate::tables::merkle::FuelBlockMerkleData)
    FuelBlockMerkleData = 8,
    /// See [`FuelBlockMerkleMetadata`](crate::tables::merkle::FuelBlockMerkleMetadata)
    FuelBlockMerkleMetadata = 9,
    /// See [`ContractsAssetsMerkleData`](crate::tables::merkle::ContractsAssetsMerkleData)
    ContractsAssetsMerkleData = 10,
    /// See [`ContractsAssetsMerkleMetadata`](crate::tables::merkle::ContractsAssetsMerkleMetadata)
    ContractsAssetsMerkleMetadata = 11,
    /// See [`ContractsStateMerkleData`](crate::tables::merkle::ContractsStateMerkleData)
    ContractsStateMerkleData = 12,
    /// See [`ContractsStateMerkleMetadata`](crate::tables::merkle::ContractsStateMerkleMetadata)
    ContractsStateMerkleMetadata = 13,
    /// See [`Messages`](crate::tables::Messages)
    Messages = 14,
    /// See [`ProcessedTransactions`](crate::tables::ProcessedTransactions)
    ProcessedTransactions = 15,
    /// See [`SealedBlockConsensus`](crate::tables::SealedBlockConsensus)
    FuelBlockConsensus = 16,
    /// See [`ConsensusParametersVersions`](crate::tables::ConsensusParametersVersions)
    ConsensusParametersVersions = 17,
    /// See [`StateTransitionBytecodeVersions`](crate::tables::StateTransitionBytecodeVersions)
    StateTransitionBytecodeVersions = 18,
    /// See [`UploadedBytecodes`](crate::tables::UploadedBytecodes)
    UploadedBytecodes = 19,
    /// See [`Blobs`](fuel_vm_private::storage::BlobData)
    Blobs = 20,

    // TODO: Remove this column and use `Metadata` column instead.
    /// Table for genesis state import progress tracking.
    GenesisMetadata = 21,
}

impl Column {
    /// The total count of variants in the enum.
    pub const COUNT: usize = <Self as strum::EnumCount>::COUNT;

    /// Returns the `usize` representation of the `Column`.
    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

impl StorageColumn for Column {
    fn name(&self) -> String {
        let str: &str = self.into();
        str.to_string()
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}
