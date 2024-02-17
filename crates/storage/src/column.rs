//! The module defines the `Column` and default tables used by the current `fuel-core` codebase.
//! In the future, the `Column` enum should contain only the required tables for the execution.
//! All other tables should live in the downstream creates in the place where they are really used.

use crate::kv_store::StorageColumn;

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
)]
pub enum Column {
    /// See [`ContractsRawCode`](crate::tables::ContractsRawCode)
    ContractsRawCode = 0,
    /// See [`ContractsInfo`](crate::tables::ContractsInfo)
    ContractsInfo = 1,
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
    /// Messages that have been spent.
    /// Existence of a key in this column means that the message has been spent.
    /// See [`SpentMessages`](crate::tables::SpentMessages)
    SpentMessages = 10,
    /// See [`ContractsAssetsMerkleData`](crate::tables::merkle::ContractsAssetsMerkleData)
    ContractsAssetsMerkleData = 11,
    /// See [`ContractsAssetsMerkleMetadata`](crate::tables::merkle::ContractsAssetsMerkleMetadata)
    ContractsAssetsMerkleMetadata = 12,
    /// See [`ContractsStateMerkleData`](crate::tables::merkle::ContractsStateMerkleData)
    ContractsStateMerkleData = 13,
    /// See [`ContractsStateMerkleMetadata`](crate::tables::merkle::ContractsStateMerkleMetadata)
    ContractsStateMerkleMetadata = 14,
    /// See [`Messages`](crate::tables::Messages)
    Messages = 15,
    /// See [`ProcessedTransactions`](crate::tables::ProcessedTransactions)
    ProcessedTransactions = 16,

    // TODO: Extract the columns below into a separate enum to not mix
    //  required columns and non-required columns. It will break `MemoryStore`
    //  and `MemoryTransactionView` because they rely on linear index incrementation.

    // Below are the tables used for p2p, block production, starting the node.
    /// The column id of metadata about the blockchain
    Metadata = 17,
    /// See [`SealedBlockConsensus`](crate::tables::SealedBlockConsensus)
    FuelBlockConsensus = 18,
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
    fn name(&self) -> &'static str {
        self.into()
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}
