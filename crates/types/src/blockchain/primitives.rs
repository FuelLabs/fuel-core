//! Primitive types

use crate::{
    fuel_crypto,
    fuel_crypto::SecretKey,
    fuel_types::Bytes32,
};
use derive_more::{
    Add,
    AsRef,
    Deref,
    Display,
    From,
    FromStr,
    Into,
    LowerHex,
    Rem,
    Sub,
    UpperHex,
};
use secrecy::{
    zeroize,
    CloneableSecret,
    DebugSecret,
};
use zeroize::Zeroize;

#[derive(Clone, Copy, Debug, Default)]
/// Empty generated fields.
pub struct Empty;

/// A cryptographically secure hash, identifying a block.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    FromStr,
    From,
    Into,
    LowerHex,
    UpperHex,
    Display,
    AsRef,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[repr(transparent)]
pub struct BlockId(Bytes32);

impl BlockId {
    /// Converts the hash into a message having the same bytes.
    pub fn into_message(self) -> fuel_crypto::Message {
        // This is safe because BlockId is a cryptographically secure hash.
        unsafe { fuel_crypto::Message::from_bytes_unchecked(*self.0) }
        // Without this, the signature would be using a hash of the id making it more
        // difficult to verify.
    }

    /// Converts the hash into a message having the same bytes.
    pub fn as_message(&self) -> &fuel_crypto::Message {
        // This is safe because BlockId is a cryptographically secure hash.
        unsafe { fuel_crypto::Message::as_ref_unchecked(self.0.as_slice()) }
        // Without this, the signature would be using a hash of the id making it more
        // difficult to verify.
    }
}

/// Block height
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    PartialOrd,
    Ord,
    Eq,
    Add,
    Sub,
    Display,
    Into,
    From,
    Deref,
    Hash,
)]
#[repr(transparent)]
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
    type Error = core::array::TryFromSliceError;

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
    /// Convert to array of big endian bytes
    pub fn to_bytes(self) -> [u8; 4] {
        self.0.to_be_bytes()
    }

    /// Convert to usize
    pub fn to_usize(self) -> usize {
        self.0 as usize
    }

    /// Convert to usize
    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

/// Block height of the data availability layer
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(
    Sub,
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
    Rem,
    Deref,
    Hash,
)]
#[rem(forward)]
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

impl core::ops::Add<u64> for DaBlockHeight {
    type Output = Self;

    fn add(self, other: u64) -> Self::Output {
        Self::from(self.0 + other)
    }
}

impl DaBlockHeight {
    /// Convert to array of big endian bytes
    pub fn to_bytes(self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    /// Convert to usize
    pub fn to_usize(self) -> usize {
        self.0 as usize
    }

    /// Convert to usize
    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }

    /// Convert to u64
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Wrapper around [`fuel_crypto::SecretKey`] to implement [`secrecy`] marker traits
#[derive(
    Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Zeroize, Deref, From,
)]
#[repr(transparent)]
pub struct SecretKeyWrapper(SecretKey);

impl CloneableSecret for SecretKeyWrapper {}
impl DebugSecret for SecretKeyWrapper {}
