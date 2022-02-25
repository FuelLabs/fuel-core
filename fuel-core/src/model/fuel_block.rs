use chrono::{DateTime, TimeZone, Utc};
use derive_more::{Add, Display, From, Into};
use fuel_tx::{crypto::Hasher, Address, Bytes32, Transaction};
use fuel_types::Word;
use serde::{Deserialize, Serialize};
use std::{
    array::TryFromSliceError,
    convert::{TryFrom, TryInto},
};

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    PartialOrd,
    Deserialize,
    Serialize,
    Add,
    Display,
    Into,
    From,
)]
pub struct BlockHeight(u32);

impl From<BlockHeight> for Vec<u8> {
    fn from(height: BlockHeight) -> Self {
        height.0.to_be_bytes().to_vec()
    }
}

impl From<u64> for BlockHeight {
    fn from(height: u64) -> Self {
        Self(height as u32)
    }
}

impl From<BlockHeight> for u64 {
    fn from(b: BlockHeight) -> Self {
        b.0 as u64
    }
}

impl TryFrom<Vec<u8>> for BlockHeight {
    type Error = TryFromSliceError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let block_height_bytes: [u8; 4] = value.as_slice().try_into()?;
        Ok(BlockHeight(u32::from_be_bytes(block_height_bytes)))
    }
}

impl From<usize> for BlockHeight {
    fn from(n: usize) -> Self {
        BlockHeight(n as u32)
    }
}

impl BlockHeight {
    pub(crate) fn to_bytes(self) -> [u8; 4] {
        self.0.to_be_bytes()
    }

    pub(crate) fn to_usize(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FuelBlockHeader {
    /// Fuel block height.
    pub height: BlockHeight,
    /// Ethereum block number.
    pub number: BlockHeight,
    /// Block header hash of the previous block.
    pub prev_hash: Bytes32,
    /// Merkle root of all previous block header hashes.
    pub prev_root: Bytes32,
    /// Merkle root of transactions.
    pub transactions_root: Bytes32,
    /// The block producer time
    pub time: DateTime<Utc>,
    /// The block producer public key
    pub producer: Address,
    /// The coinbase award for the block producer
    pub coinbase: Word,
}

impl FuelBlockHeader {
    pub fn id(&self) -> Bytes32 {
        let mut hasher = Hasher::default();
        hasher.input(&self.height.to_bytes()[..]);
        hasher.input(&self.number.to_bytes()[..]);
        hasher.input(self.prev_hash.as_ref());
        hasher.input(self.prev_root.as_ref());
        hasher.input(self.transactions_root.as_ref());
        hasher.input(self.time.timestamp_millis().to_be_bytes());
        hasher.input(self.producer.as_ref());
        hasher.input(&self.coinbase.to_be_bytes()[..]);
        hasher.digest()
    }
}

impl Default for FuelBlockHeader {
    fn default() -> Self {
        Self {
            height: 0u32.into(),
            number: 0u32.into(),
            prev_hash: Default::default(),
            time: Utc.timestamp(0, 0),
            producer: Default::default(),
            transactions_root: Default::default(),
            prev_root: Default::default(),
            coinbase: 0,
        }
    }
}

/// The compact representation of a block used in the database
#[derive(Clone, Debug, Deserialize, Default, Serialize)]
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
#[derive(Clone, Debug, Deserialize, Default, Serialize)]
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
