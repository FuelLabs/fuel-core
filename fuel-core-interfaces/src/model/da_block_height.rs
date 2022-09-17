use derive_more::{
    Add,
    Deref,
    Display,
    From,
    Into,
};

use std::ops::{Add, Sub, Rem};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    PartialOrd,
    Eq,
    Add,
    Ord,
    Display,
    Into,
    From,
    Deref,
    Hash,
)]
pub struct DaBlockHeight(pub u64);

impl From<DaBlockHeight> for Vec<u8> {
    fn from(height: DaBlockHeight) -> Self {
        height.0.to_be_bytes().to_vec()
    }
}

impl From<usize> for DaBlockHeight {
    fn from(n: usize) -> Self {
        DaBlockHeight(n as u64)
    }
}

impl Add<u64> for DaBlockHeight {
    type Output = Self;

    fn add(self, other: u64) -> Self::Output {
        Self::from(self.0 + other)
    }
}

impl Sub for DaBlockHeight {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self::from(self.0 - other.as_u64())
    }
}

impl Rem for DaBlockHeight {
    type Output = Self;

    fn rem(self, other: Self) -> Self::Output {
        Self::from(self.0 % other.as_u64())
    }
}

impl DaBlockHeight {
    pub fn to_bytes(self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    pub fn to_usize(self) -> usize {
        self.0 as usize
    }

    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}
