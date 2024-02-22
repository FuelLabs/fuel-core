use fuel_core_storage::kv_store::StorageColumn;

pub mod blocks;
pub mod coins;
pub mod messages;
pub mod transactions;

/// GraphQL database tables column ids to the corresponding [`fuel_core_storage::Mappable`] table.
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
    /// The column id of metadata about the blockchain
    Metadata = 0,
    /// The column of the table that stores `true` if `owner` owns `Coin` with `coin_id`
    OwnedCoins = 1,
    /// Transaction id to current status
    TransactionStatus = 2,
    /// The column of the table of all `owner`'s transactions
    TransactionsByOwnerBlockIdx = 3,
    /// The column of the table that stores `true` if `owner` owns `Message` with `message_id`
    OwnedMessageIds = 4,
    /// The column of the table that stores statistic about the blockchain.
    Statistic = 5,
    /// See [`blocks::FuelBlockIdsToHeights`]
    FuelBlockIdsToHeights = 6,
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
