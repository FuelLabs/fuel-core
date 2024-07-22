//! The module contains the traits for encoding and decoding the types(a.k.a Codec).
//! It implements common codecs and encoders, but it is always possible to define own codecs.

use crate::kv_store::Value;
use core::ops::Deref;

#[cfg(feature = "std")]
use std::borrow::Cow;

#[cfg(not(feature = "std"))]
use alloc::borrow::Cow;

pub mod manual;
pub mod postcard;
pub mod primitive;
pub mod raw;

/// The trait is usually implemented by the encoder that stores serialized objects.
pub trait Encoder {
    /// Returns the serialized object as a slice.
    fn as_bytes(&self) -> Cow<[u8]>;
}

/// The trait encodes the type to the bytes and passes it to the `Encoder`,
/// which stores it and provides a reference to it. That allows gives more
/// flexibility and more performant encoding, allowing the use of slices and arrays
/// instead of vectors in some cases. Since the [`Encoder`] returns `Cow<[u8]>`,
/// it is always possible to take ownership of the serialized value.
pub trait Encode<T: ?Sized> {
    /// The encoder type that stores serialized object.
    type Encoder<'a>: Encoder
    where
        T: 'a;

    /// Encodes the object to the bytes and passes it to the `Encoder`.
    fn encode(t: &T) -> Self::Encoder<'_>;

    /// Returns the serialized object as an [`Value`].
    fn encode_as_value(t: &T) -> Value {
        Value::new(Self::encode(t).as_bytes().into_owned())
    }
}

/// The trait decodes the type from the bytes.
pub trait Decode<T> {
    /// Decodes the type `T` from the bytes.
    fn decode(bytes: &[u8]) -> anyhow::Result<T>;

    /// Decodes the type `T` from the [`Value`].
    fn decode_from_value(value: Value) -> anyhow::Result<T> {
        Self::decode(value.deref())
    }
}

impl<'a> Encoder for Cow<'a, [u8]> {
    fn as_bytes(&self) -> Cow<[u8]> {
        match self {
            Cow::Borrowed(borrowed) => Cow::Borrowed(borrowed),
            Cow::Owned(owned) => Cow::Borrowed(owned.as_ref()),
        }
    }
}

impl<const SIZE: usize> Encoder for [u8; SIZE] {
    fn as_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(self.as_slice())
    }
}
