use chrono::{DateTime, Utc};
//use derive_more::{Add, Display, From, Into};
use fuel_crypto::Hasher;
use fuel_tx::{Address, AssetId, Bytes32, Transaction, Word};
use std::{
    array::TryFromSliceError,
    convert::{TryFrom, TryInto},
    ops::Deref,
};

#[derive(Copy, Clone, Debug, Default, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde-types", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockHeight(pub u32);

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
    pub fn to_bytes(self) -> [u8; 4] {
        self.0.to_be_bytes()
    }

    pub fn to_usize(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug)] //, Deserialize, Serialize)]
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

/// The compact representation of a block used in the database
#[derive(Clone, Debug)] //, Deserialize, Default, Serialize)]
pub struct FuelBlockDb {
    pub headers: FuelBlockHeader,
    pub transactions: Vec<Bytes32>,
}

impl FuelBlockDb {
    pub fn id(&self) -> Bytes32 {
        self.headers.id()
    }
}
#[derive(Clone, Debug)]
pub struct FuelBlockConsensus {
    pub required_stake: u64,
    pub validators: Vec<Address>,
    pub stakes: Vec<u64>,
    pub signatures: Vec<Bytes32>,
}

#[derive(Clone, Debug)]
pub struct SealedFuelBlock {
    pub block: FuelBlock,
    pub consensus: FuelBlockConsensus,
}

impl Deref for SealedFuelBlock {
    type Target = FuelBlock;

    fn deref(&self) -> &FuelBlock {
        &self.block
    }
}

/// Fuel block with all transaction data included
#[derive(Clone, Debug)] //, Deserialize, Default, Serialize)]
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

    pub fn transaction_data_lenght(&self) -> usize {
        self.transactions.len() * 100
    }

    pub fn transaction_data_hash(&self) -> Bytes32 {
        Bytes32::zeroed()
    }

    pub fn validator_set_hash(&self) -> Bytes32 {
        Bytes32::zeroed()
    }

    pub fn transaction_sum(&self) -> usize {
        0
    }

    pub fn withdrawals(&self) -> Vec<(Address, Word, AssetId)> {
        vec![]
    }

    pub fn withdrawals_root(&self) -> Bytes32 {
        Bytes32::zeroed()
    }
}
