pub use super::BlockHeight;
use chrono::{DateTime, TimeZone, Utc};
use fuel_crypto::Hasher;
use fuel_tx::{Address, Bytes32, Transaction};

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde-types", derive(serde::Serialize, serde::Deserialize))]
pub struct FuelBlockHeader {
    /// Fuel block height.
    pub height: BlockHeight,
    /// The layer 1 height of deposits and events to include since the last layer 1 block number.
    /// This is not meant to represent the layer 1 block this was committed to. Validators will need
    /// to have some rules in place to ensure the block number was chosen in a reasonable way. For
    /// example, they should verify that the block number satisfies the finality requirements of the
    /// layer 1 chain. They should also verify that the block number isn't too stale and is increasing.
    /// Some similar concerns are noted in this issue: https://github.com/FuelLabs/fuel-specs/issues/220
    pub number: BlockHeight,
    /// Block header hash of the previous block.
    pub parent_hash: Bytes32,
    /// Merkle root of all previous block header hashes.
    pub prev_root: Bytes32,
    /// Merkle root of transactions.
    pub transactions_root: Bytes32,
    /// The block producer time
    pub time: DateTime<Utc>,
    /// The block producer public key
    pub producer: Address,
}

impl FuelBlockHeader {
    pub fn id(&self) -> Bytes32 {
        let mut hasher = Hasher::default();
        hasher.input(&self.height.to_bytes()[..]);
        hasher.input(&self.number.to_bytes()[..]);
        hasher.input(self.parent_hash.as_ref());
        hasher.input(self.prev_root.as_ref());
        hasher.input(self.transactions_root.as_ref());
        hasher.input(self.time.timestamp_millis().to_be_bytes());
        hasher.input(self.producer.as_ref());
        hasher.digest()
    }
}

impl Default for FuelBlockHeader {
    fn default() -> Self {
        Self {
            height: 0u32.into(),
            number: 0u32.into(),
            parent_hash: Default::default(),
            time: Utc.timestamp(0, 0),
            producer: Default::default(),
            transactions_root: Default::default(),
            prev_root: Default::default(),
        }
    }
}

/// The compact representation of a block used in the database
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde-types", derive(serde::Serialize, serde::Deserialize))]
pub struct FuelBlockDb {
    pub headers: FuelBlockHeader,
    pub transactions: Vec<Bytes32>,
}

impl FuelBlockDb {
    pub fn id(&self) -> Bytes32 {
        self.headers.id()
    }
}

/// Fuel block with all transaction data included
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde-types", derive(serde::Serialize, serde::Deserialize))]
pub struct FuelBlock {
    pub header: FuelBlockHeader,
    pub transactions: Vec<Transaction>,
}

impl FuelBlock {
    pub fn id(&self) -> Bytes32 {
        self.header.id()
    }

    pub fn to_db_block(&self) -> FuelBlockDb {
        FuelBlockDb {
            headers: self.header.clone(),
            transactions: self.transactions.iter().map(|tx| tx.id()).collect(),
        }
    }
}
