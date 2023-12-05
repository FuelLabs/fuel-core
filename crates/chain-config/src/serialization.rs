use core::fmt;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::{
        bytes::WORD_SIZE,
        BlockHeight,
    },
};
use serde::{
    de::Error,
    Deserializer,
    Serializer,
};
use serde_with::{
    DeserializeAs,
    SerializeAs,
};
use std::convert::TryFrom;

/// Used for primitive number types which don't implement AsRef or TryFrom<&[u8]>
pub(crate) struct HexNumber;

impl SerializeAs<BlockHeight> for HexNumber {
    fn serialize_as<S>(value: &BlockHeight, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let number: u32 = (*value).into();
        HexNumber::serialize_as(&number, serializer)
    }
}

impl<'de> DeserializeAs<'de, BlockHeight> for HexNumber {
    fn deserialize_as<D>(deserializer: D) -> Result<BlockHeight, D::Error>
    where
        D: Deserializer<'de>,
    {
        let number: u32 = HexNumber::deserialize_as(deserializer)?;
        Ok(number.into())
    }
}

impl SerializeAs<DaBlockHeight> for HexNumber {
    fn serialize_as<S>(value: &DaBlockHeight, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let number: u64 = (*value).into();
        HexNumber::serialize_as(&number, serializer)
    }
}

impl<'de> DeserializeAs<'de, DaBlockHeight> for HexNumber {
    fn deserialize_as<D>(deserializer: D) -> Result<DaBlockHeight, D::Error>
    where
        D: Deserializer<'de>,
    {
        let number: u64 = HexNumber::deserialize_as(deserializer)?;
        Ok(number.into())
    }
}

pub(crate) struct HexType;

impl<T: AsRef<[u8]>> SerializeAs<T> for HexType {
    fn serialize_as<S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serde_hex::serialize(value, serializer)
    }
}

impl<'de, T, E> DeserializeAs<'de, T> for HexType
where
    for<'a> T: TryFrom<&'a [u8], Error = E>,
    E: fmt::Display,
{
    fn deserialize_as<D>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
    {
        serde_hex::deserialize(deserializer)
    }
}

pub mod serde_hex {
    use core::fmt;
    use hex::{
        FromHex,
        ToHex,
    };
    use serde::{
        de::Error,
        Deserializer,
        Serializer,
    };
    use std::convert::TryFrom;

    pub fn serialize<T, S>(target: T, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: ToHex,
    {
        let s = format!("0x{}", target.encode_hex::<String>());
        ser.serialize_str(&s)
    }

    pub fn deserialize<'de, T, E, D>(des: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        for<'a> T: TryFrom<&'a [u8], Error = E>,
        E: fmt::Display,
    {
        let raw_string: String = serde::Deserialize::deserialize(des)?;
        let stripped_prefix = raw_string.trim_start_matches("0x");
        let bytes: Vec<u8> =
            FromHex::from_hex(stripped_prefix).map_err(D::Error::custom)?;
        let result = T::try_from(bytes.as_slice()).map_err(D::Error::custom)?;
        Ok(result)
    }
}

macro_rules! impl_hex_number {
    ($i:ident) => {
        impl SerializeAs<$i> for HexNumber {
            fn serialize_as<S>(value: &$i, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let bytes = value.to_be_bytes();
                serde_hex::serialize(bytes, serializer)
            }
        }

        impl<'de> DeserializeAs<'de, $i> for HexNumber {
            fn deserialize_as<D>(deserializer: D) -> Result<$i, D::Error>
            where
                D: Deserializer<'de>,
            {
                const SIZE: usize = core::mem::size_of::<$i>();
                let mut bytes: Vec<u8> = serde_hex::deserialize(deserializer)?;
                let pad =
                    SIZE.checked_sub(bytes.len())
                        .ok_or(D::Error::custom(format!(
                            "value cant exceed {WORD_SIZE} bytes"
                        )))?;

                if pad != 0 {
                    // pad if length < word size
                    bytes = (0..pad).map(|_| 0u8).chain(bytes.into_iter()).collect();
                }

                // We've already verified the bytes.len == WORD_SIZE, force the conversion here.
                Ok($i::from_be_bytes(
                    bytes.try_into().expect("byte lengths checked"),
                ))
            }
        }
    };
}

impl_hex_number!(u8);
impl_hex_number!(u16);
impl_hex_number!(u32);
impl_hex_number!(u64);
