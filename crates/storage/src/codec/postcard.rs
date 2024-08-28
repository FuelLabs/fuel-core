//! The module contains the implementation of the `Postcard` codec.
//! Any type that implements `serde::Serialize` and `serde::Deserialize`
//! can use the `Postcard` codec to be encoded/decoded into/from bytes.
//! The `serde` serialization and deserialization add their own overhead,
//! so this codec shouldn't be used for simple types.

use crate::codec::{
    Decode,
    Encode,
};

#[cfg(feature = "std")]
use std::borrow::Cow;

#[cfg(not(feature = "std"))]
use alloc::borrow::Cow;

/// The codec is used to serialized/deserialized types that supports `serde::Serialize` and `serde::Deserialize`.
pub struct Postcard;

impl<T> Encode<T> for Postcard
where
    T: ?Sized + serde::Serialize,
{
    type Encoder<'a> = Cow<'a, [u8]> where T: 'a;

    fn encode(value: &T) -> Self::Encoder<'_> {
        Cow::Owned(postcard::to_allocvec(value).expect(
            "It should be impossible to fail unless serialization is not implemented, which is not true for our types.",
        ))
    }
}

impl<T> Decode<T> for Postcard
where
    T: serde::de::DeserializeOwned,
{
    fn decode(bytes: &[u8]) -> anyhow::Result<T> {
        Ok(postcard::from_bytes(bytes)?)
    }
}
