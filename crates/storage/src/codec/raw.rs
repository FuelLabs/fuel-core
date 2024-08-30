//! The module contains the implementation of the `Raw` codec.
//! The codec is used for types that are already represented by bytes
//! and can be deserialized into bytes-based objects.

use crate::codec::{
    Decode,
    Encode,
};

#[cfg(feature = "std")]
use std::borrow::Cow;

#[cfg(not(feature = "std"))]
use alloc::borrow::Cow;

/// The codec is used for types that are already represented by bytes.
pub struct Raw;

impl<T> Encode<T> for Raw
where
    T: ?Sized + AsRef<[u8]>,
{
    type Encoder<'a> = Cow<'a, [u8]> where T: 'a;

    fn encode(t: &T) -> Self::Encoder<'_> {
        Cow::Borrowed(t.as_ref())
    }
}

impl<T> Decode<T> for Raw
where
    for<'a> T: TryFrom<&'a [u8]>,
{
    fn decode(bytes: &[u8]) -> anyhow::Result<T> {
        T::try_from(bytes).map_err(|_| anyhow::anyhow!("Unable to decode bytes"))
    }
}
