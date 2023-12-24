use crate::kv_store::Value;
use std::{
    borrow::Cow,
    ops::Deref,
};

pub mod manual;
pub mod postcard;
pub mod primitive;
pub mod raw;

pub trait Encoder {
    fn as_bytes(&self) -> Cow<[u8]>;
}

pub trait Encode<T: ?Sized> {
    type Encoder<'a>: Encoder
    where
        T: 'a;

    fn encode(t: &T) -> Self::Encoder<'_>;

    fn encode_as_value(t: &T) -> Value {
        Value::new(Self::encode(t).as_bytes().into_owned())
    }
}

pub trait Decode<T> {
    fn decode(bytes: &[u8]) -> anyhow::Result<T>;

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
