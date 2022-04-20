use derive_more::{Add, Display, From, Into};
use std::{
    array::TryFromSliceError,
    convert::{TryFrom, TryInto},
};

#[cfg_attr(feature = "serde-types", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Default, PartialEq, PartialOrd, Add, Display, Into, From)]
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
    pub fn to_bytes(self) -> [u8; 4] {
        self.0.to_be_bytes()
    }

    pub fn to_usize(self) -> usize {
        self.0 as usize
    }
}
