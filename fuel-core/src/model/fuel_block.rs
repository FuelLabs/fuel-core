use chrono::{DateTime, TimeZone, Utc};
use derive_more::{Add, Display, From, Into};
use fuel_tx::{crypto::Hasher, Address, Bytes32};
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
pub struct FuelBlock {
    pub fuel_height: BlockHeight,
    pub transactions: Vec<Bytes32>,
    pub time: DateTime<Utc>,
    pub producer: Address,
}

impl Default for FuelBlock {
    fn default() -> Self {
        Self {
            fuel_height: 0u32.into(),
            transactions: vec![],
            time: Utc.timestamp(0, 0),
            producer: Default::default(),
        }
    }
}

impl FuelBlock {
    pub fn id(&self) -> Bytes32 {
        let mut hasher = Hasher::from_iter(&self.transactions);
        hasher.input(&self.fuel_height.to_bytes()[..]);
        hasher.input(self.time.timestamp_millis().to_be_bytes());
        hasher.input(self.producer.as_ref());
        hasher.digest()
    }
}
