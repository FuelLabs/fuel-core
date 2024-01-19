//! The module contains the implementation of the `Postcard` codec.
//! Any type that implements `serde::Serialize` and `serde::Deserialize`
//! can use the `Postcard` codec to be encoded/decoded into/from bytes.
//! The `serde` serialization and deserialization add their own overhead,
//! so this codec shouldn't be used for simple types.

use crate::codec::{
    Decode,
    Encode,
};
use std::borrow::Cow;

/// The codec is used to serialized/deserialized types that supports `serde::Serialize` and `serde::Deserialize`.
pub struct Postcard;

impl<K> Encode<K> for Postcard
where
    K: ?Sized + serde::Serialize,
{
    type Encoder<'a> = Cow<'a, [u8]> where K: 'a;

    fn encode(value: &K) -> Self::Encoder<'_> {
        Cow::Owned(postcard::to_allocvec(value).expect(
            "It should be impossible to fail unless serialization is not implemented, which is not true for our types.",
        ))
    }
}

impl<V> Decode<V> for Postcard
where
    V: serde::de::DeserializeOwned,
{
    fn decode(bytes: &[u8]) -> anyhow::Result<V> {
        Ok(postcard::from_bytes(bytes)?)
    }
}
