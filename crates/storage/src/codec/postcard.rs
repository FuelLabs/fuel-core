use crate::codec::{
    Decode,
    Encode,
};
use std::borrow::Cow;

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
