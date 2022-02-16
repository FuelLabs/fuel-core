use chrono::{DateTime, TimeZone, Utc};
use derive_more::{Add, Display, From, Into};
use fuel_tx::{crypto::Hasher, Address, Bytes32, Transaction};
use fuel_types::Word;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{
    array::TryFromSliceError,
    convert::{TryFrom, TryInto},
    iter::FromIterator,
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

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct TransactionCommitment {
    pub sum: Word,
    pub root: Bytes32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FuelBlockHeaders {
    pub fuel_height: BlockHeight,
    pub time: DateTime<Utc>,
    pub producer: Address,
    // TODO: integrate with fuel-merkle
    pub transactions_commitment: TransactionCommitment,
}

impl FuelBlockHeaders {
    pub fn id(&self, transaction_ids: &[Bytes32]) -> Bytes32 {
        let mut hasher = Hasher::from_iter(transaction_ids);
        hasher.input(&self.fuel_height.to_bytes()[..]);
        hasher.input(self.time.timestamp_millis().to_be_bytes());
        hasher.input(self.producer.as_ref());
        hasher.input(self.transactions_commitment.sum.to_be_bytes());
        hasher.input(self.transactions_commitment.root.as_ref());
        hasher.digest()
    }
}

impl Default for FuelBlockHeaders {
    fn default() -> Self {
        Self {
            fuel_height: 0u32.into(),
            time: Utc.timestamp(0, 0),
            producer: Default::default(),
            transactions_commitment: Default::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Default, Serialize)]
pub struct FuelBlockLight {
    pub headers: FuelBlockHeaders,
    pub transactions: Vec<Bytes32>,
}

impl FuelBlockLight {
    pub fn id(&self) -> Bytes32 {
        self.headers.id(&self.transactions)
    }
}

/// Fuel block with all transaction data included
#[derive(Clone, Debug, Deserialize, Default, Serialize)]
pub struct FuelBlockFull {
    pub headers: FuelBlockHeaders,
    pub transactions: Vec<Transaction>,
}

impl FuelBlockFull {
    pub fn id(&self) -> Bytes32 {
        self.headers
            .id(&self.transactions.iter().map(|tx| tx.id()).collect_vec())
    }

    pub fn as_light(&self) -> FuelBlockLight {
        FuelBlockLight {
            headers: self.headers.clone(),
            transactions: self.transactions.iter().map(|tx| tx.id()).collect(),
        }
    }
}
