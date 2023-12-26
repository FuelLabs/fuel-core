//! The module contains the implementation of the `Raw` codec.
//! The codec is used for types that are already represented by bytes
//! and can be deserialized into bytes-based objects.

use crate::codec::{
    Decode,
    Encode,
};
use std::borrow::Cow;

/// The codec is used for types that are already represented by bytes.
pub struct Raw;

impl<K> Encode<K> for Raw
where
    K: ?Sized + AsRef<[u8]>,
{
    type Encoder<'a> = Cow<'a, [u8]> where K: 'a;

    fn encode(t: &K) -> Self::Encoder<'_> {
        Cow::Borrowed(t.as_ref())
    }
}

impl<V> Decode<V> for Raw
where
    for<'a> V: TryFrom<&'a [u8]>,
{
    fn decode(bytes: &[u8]) -> anyhow::Result<V> {
        V::try_from(bytes).map_err(|_| anyhow::anyhow!("Unable to decode bytes"))
    }
}
