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
        fuel_crypto::Message::from_bytes(*self.0)
    }

    /// Converts the hash into a message having the same bytes.
    pub fn as_message(&self) -> &fuel_crypto::Message {
        fuel_crypto::Message::from_bytes_ref(&self.0)
    }

    /// Represents `BlockId` as slice of bytes.
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl AsRef<[u8]> for BlockId {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
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

impl DaBlockHeight {
    /// Convert to array of big endian bytes
    pub fn to_bytes(self) -> [u8; 8] {
        self.0.to_be_bytes()
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

impl From<BlockId> for [u8; 32] {
    fn from(id: BlockId) -> Self {
        id.0.into()
    }
}

impl From<[u8; 32]> for BlockId {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes.into())
    }
}
