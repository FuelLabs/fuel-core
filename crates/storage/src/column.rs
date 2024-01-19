//! The module defines the `Column` and default tables used by the current `fuel-core` codebase.
//! In the future, the `Column` enum should contain only the required tables for the execution.
//! All other tables should live in the downstream creates in the place where they are really used.

use crate::kv_store::StorageColumn;

/// Helper macro to generate the `Column` enum and its implementation for `as_u32` method.
macro_rules! column_definition {
    ($(#[$meta:meta])* $vis:vis enum $name:ident {
        $(#[$complex_meta:meta])* $complex_variants:ident($body:ident),
        $($(#[$const_meta:meta])* $const_variants:ident = $const_number:expr,)*
    }) => {
        $(#[$meta])*
        $vis enum $name {
            $($(#[$const_meta])* $const_variants = $const_number,)*
            $(#[$complex_meta])* $complex_variants($body),
        }

        impl $name {
            /// Returns the `u32` representation of the `Self`.
            pub fn as_u32(&self) -> u32 {
                match self {
                    $($name::$const_variants => $const_number,)*
                    $name::$complex_variants(foreign) => foreign.id,
                }
            }
        }
    }
}

column_definition! {
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
        /// The foreign column is not related to the required tables.
        ForeignColumn(ForeignColumn),

        // Tables that are required for the state transition and fraud proving.

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
        /// See [`Receipts`](crate::tables::Receipts)
        Receipts = 18,
        /// See `FuelBlockSecondaryKeyBlockHeights`
        FuelBlockSecondaryKeyBlockHeights = 19,
        /// See [`SealedBlockConsensus`](crate::tables::SealedBlockConsensus)
        FuelBlockConsensus = 20,
        /// Metadata for the relayer
        /// See `RelayerMetadata`
        RelayerMetadata = 21,

        // Below are not required tables. They are used for API and may be removed or moved to another place in the future.

        /// The column of the table that stores `true` if `owner` owns `Coin` with `coin_id`
        OwnedCoins = 22,
        /// Transaction id to current status
        TransactionStatus = 23,
        /// The column of the table of all `owner`'s transactions
        TransactionsByOwnerBlockIdx = 24,
        /// The column of the table that stores `true` if `owner` owns `Message` with `message_id`
        OwnedMessageIds = 25,
    }
}

impl Column {
    /// The total count of variants in the enum.
    pub const COUNT: usize = <Self as strum::EnumCount>::COUNT;

    /// Returns the `usize` representation of the `Column`.
    pub fn as_usize(&self) -> usize {
        self.as_u32() as usize
    }
}

impl StorageColumn for Column {
    fn name(&self) -> &'static str {
        match self {
            Column::ForeignColumn(foreign) => foreign.name,
            variant => variant.into(),
        }
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}

/// The foreign column is not related to the required tables.
/// It can be used to extend the database with additional tables.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ForeignColumn {
    id: u32,
    name: &'static str,
}

impl ForeignColumn {
    /// Creates the foreign column ensuring that the id and name
    /// are not already used by the [`Column`] required tables.
    pub fn new(id: u32, name: &'static str) -> anyhow::Result<Self> {
        for column in enum_iterator::all::<Column>() {
            if column.id() == id {
                anyhow::bail!("Column id {} is already used by {}", id, column.name());
            }
            if column.name() == name {
                anyhow::bail!(
                    "Column name {} is already used by {}",
                    name,
                    column.name()
                );
            }
        }
        Ok(Self { id, name })
    }
}

/// It is required to implement iteration over the variants of the enum.
/// The `ForeignColumn` is not iterable, so we implement the `Sequence` trait
/// to do nothing.
impl enum_iterator::Sequence for ForeignColumn {
    const CARDINALITY: usize = 0;

    fn next(&self) -> Option<Self> {
        None
    }

    fn previous(&self) -> Option<Self> {
        None
    }

    fn first() -> Option<Self> {
        None
    }

    fn last() -> Option<Self> {
        None
    }
}
