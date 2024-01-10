use core::fmt;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
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
        if serializer.is_human_readable() {
            serde_hex::serialize(value, serializer)
        } else {
            Ok(serde::Serialize::serialize(value.as_ref(), serializer)?)
        }
    }
}

// TODO: This can be optimized for storage if this implementation was specialized for each `key!`
// generated type since postcard wouldn't need to serialize the length of the bytes. A common trait
// currently implemented by `key!` is Deref<Target = [u8; NUM_BYTES]> so that could be used in a pinch.
impl<'de, T, E> DeserializeAs<'de, T> for HexType
where
    for<'a> T: TryFrom<&'a [u8], Error = E>,
    E: fmt::Display,
{
    fn deserialize_as<D>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            serde_hex::deserialize(deserializer)
        } else {
            let bytes: &[u8] = serde::Deserialize::deserialize(deserializer)?;
            Ok(bytes.try_into().map_err(D::Error::custom)?)
        }
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
                if serializer.is_human_readable() {
                    let bytes = value.to_be_bytes();
                    serde_hex::serialize(bytes, serializer)
                } else {
                    Ok(serde::Serialize::serialize(value, serializer)?)
                }
            }
        }

        impl<'de> DeserializeAs<'de, $i> for HexNumber {
            fn deserialize_as<D>(deserializer: D) -> Result<$i, D::Error>
            where
                D: Deserializer<'de>,
            {
                const SIZE: usize = core::mem::size_of::<$i>();
                if deserializer.is_human_readable() {
                    let mut bytes: Vec<u8> = serde_hex::deserialize(deserializer)?;
                    let pad = SIZE.checked_sub(bytes.len()).ok_or(D::Error::custom(
                        format!("value cant exceed {SIZE} bytes"),
                    ))?;

                    if pad != 0 {
                        // pad if length < word size
                        bytes = (0..pad).map(|_| 0u8).chain(bytes.into_iter()).collect();
                    }

                    // We've already verified the bytes.len == WORD_SIZE, force the conversion here.
                    Ok($i::from_be_bytes(
                        bytes.try_into().expect("byte lengths checked"),
                    ))
                } else {
                    let val: $i = serde::Deserialize::deserialize(deserializer)?;
                    Ok(val)
                }
            }
        }
    };
}

impl_hex_number!(u8);
impl_hex_number!(u16);
impl_hex_number!(u32);
impl_hex_number!(u64);

#[cfg(test)]
mod tests {

    use super::HexType;
    use crate::serialization::HexNumber;
    use fuel_core_types::{
        fuel_types::{
            Address,
            AssetId,
            Bytes32,
            ContractId,
            Nonce,
        },
        fuel_vm::Salt,
    };
    use postcard::ser_flavors::{
        AllocVec,
        Flavor,
    };
    use serde::de::DeserializeOwned;
    use serde_json::de::StrRead;
    use serde_with::{
        DeserializeAs,
        SerializeAs,
    };
    use test_case::test_case;

    #[test_case(u8::MIN, "\"0x00\""; "u8::MIN")]
    #[test_case(u8::MAX, "\"0xff\""; "u8::MAX")]
    #[test_case(u16::MIN, "\"0x0000\""; "u16::MIN")]
    #[test_case(u16::MAX, "\"0xffff\""; "u16::MAX")]
    #[test_case(u32::MIN, "\"0x00000000\""; "u32::MIN")]
    #[test_case(u32::MAX, "\"0xffffffff\""; "u32::MAX")]
    #[test_case(u64::MIN, "\"0x0000000000000000\""; "u64::MIN")]
    #[test_case(u64::MAX, "\"0xffffffffffffffff\""; "u64::MAX")]
    fn encodes_hex_num_human_readable<T>(num: T, expectation: &str)
    where
        HexNumber: SerializeAs<T>,
    {
        // given
        let mut buf = vec![];
        let mut serializer = serde_json::Serializer::new(&mut buf);

        // when
        <HexNumber as SerializeAs<T>>::serialize_as(&num, &mut serializer).unwrap();

        // then
        let actual = String::from_utf8(buf).unwrap();
        assert_eq!(actual, expectation);
    }

    #[test_case(u8::MIN, "\"0x00\""; "u8::MIN")]
    #[test_case(u8::MAX, "\"0xff\""; "u8::MAX")]
    #[test_case(u16::MIN, "\"0x0000\""; "u16::MIN")]
    #[test_case(u16::MAX, "\"0xffff\""; "u16::MAX")]
    #[test_case(u32::MIN, "\"0x00000000\""; "u32::MIN")]
    #[test_case(u32::MAX, "\"0xffffffff\""; "u32::MAX")]
    #[test_case(u64::MIN, "\"0x0000000000000000\""; "u64::MIN")]
    #[test_case(u64::MAX, "\"0xffffffffffffffff\""; "u64::MAX")]
    fn decodes_hex_num_human_readable<T>(expectation: T, encoded: &'static str)
    where
        HexNumber: DeserializeAs<'static, T>,
        T: PartialEq + std::fmt::Debug,
    {
        // given
        let mut deserializer = serde_json::Deserializer::new(StrRead::new(encoded));

        // when
        let decoded =
            <HexNumber as DeserializeAs<T>>::deserialize_as(&mut deserializer).unwrap();

        // then
        assert_eq!(decoded, expectation);
    }

    #[test_case(u8::MIN; "u8::MIN")]
    #[test_case(u8::MAX; "u8::MAX")]
    #[test_case(u16::MIN; "u16::MIN")]
    #[test_case(u16::MAX; "u16::MAX")]
    #[test_case(u32::MIN; "u32::MIN")]
    #[test_case(u32::MAX; "u32::MAX")]
    #[test_case(u64::MIN; "u64::MIN")]
    #[test_case(u64::MAX; "u64::MAX")]
    fn encodes_hex_num_machine_readable<T>(num: T)
    where
        HexNumber: SerializeAs<T>,
        T: DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        // given
        let mut serializer = postcard::Serializer {
            output: AllocVec::default(),
        };

        // when
        <HexNumber as SerializeAs<T>>::serialize_as(&num, &mut serializer).unwrap();

        // then
        let bytes = serializer.output.finalize().unwrap();
        let (val, remaining): (T, &[u8]) = postcard::take_from_bytes(&bytes).unwrap();
        assert_eq!(val, num);
        assert!(remaining.is_empty());
    }

    #[test_case(u8::MIN; "u8::MIN")]
    #[test_case(u8::MAX; "u8::MAX")]
    #[test_case(u16::MIN; "u16::MIN")]
    #[test_case(u16::MAX; "u16::MAX")]
    #[test_case(u32::MIN; "u32::MIN")]
    #[test_case(u32::MAX; "u32::MAX")]
    #[test_case(u64::MIN; "u64::MIN")]
    #[test_case(u64::MAX; "u64::MAX")]
    fn decodes_hex_num_machine_readable<T>(num: T)
    where
        for<'a> HexNumber: DeserializeAs<'a, T>,
        T: PartialEq + std::fmt::Debug + serde::Serialize,
    {
        // given
        let expected_bytes = postcard::to_stdvec(&num).unwrap();
        let mut deserializer = postcard::Deserializer::from_bytes(&expected_bytes);

        // when
        let decoded =
            <HexNumber as DeserializeAs<T>>::deserialize_as(&mut deserializer).unwrap();

        // then
        assert_eq!(decoded, num);
    }

    const BYTES: [u8; 32] = [1u8; 32];
    const HR_EXPECTATION: &str =
        "\"0x0101010101010101010101010101010101010101010101010101010101010101\"";
    #[test_case(Bytes32::new(BYTES), HR_EXPECTATION; "Bytes32")]
    #[test_case(Address::new(BYTES), HR_EXPECTATION; "Address")]
    #[test_case(AssetId::new(BYTES), HR_EXPECTATION; "AssetId")]
    #[test_case(Nonce::new(BYTES), HR_EXPECTATION; "Nonce")]
    #[test_case(Vec::from(BYTES), HR_EXPECTATION; "Vec<u8>")]
    #[test_case(ContractId::new(BYTES), HR_EXPECTATION; "ContractId")]
    #[test_case(Salt::new(BYTES), HR_EXPECTATION; "Salt")]
    fn encodes_hex_type_as_human_readable<T>(value: T, expectation: &str)
    where
        HexType: SerializeAs<T>,
    {
        // given
        let mut buf = vec![];
        let mut serializer = serde_json::Serializer::new(&mut buf);

        // when
        <HexType as SerializeAs<T>>::serialize_as(&value, &mut serializer).unwrap();

        // then
        let actual = String::from_utf8(buf).unwrap();
        assert_eq!(actual, expectation);
    }

    #[test_case(Bytes32::new(BYTES), HR_EXPECTATION; "Bytes32")]
    #[test_case(Address::new(BYTES), HR_EXPECTATION; "Address")]
    #[test_case(AssetId::new(BYTES), HR_EXPECTATION; "AssetId")]
    #[test_case(Nonce::new(BYTES), HR_EXPECTATION; "Nonce")]
    #[test_case(Vec::from(BYTES), HR_EXPECTATION; "Vec<u8>")]
    #[test_case(ContractId::new(BYTES), HR_EXPECTATION; "ContractId")]
    #[test_case(Salt::new(BYTES), HR_EXPECTATION; "Salt")]
    fn decodes_hex_type_as_human_readable<T>(expectation: T, serialized: &'static str)
    where
        HexType: DeserializeAs<'static, T>,
        T: std::fmt::Debug + PartialEq,
    {
        // given
        let mut deserializer = serde_json::Deserializer::new(StrRead::new(serialized));

        // when
        let decoded =
            <HexType as DeserializeAs<T>>::deserialize_as(&mut deserializer).unwrap();

        // then
        assert_eq!(decoded, expectation);
    }

    #[test_case(Bytes32::new(BYTES); "Bytes32")]
    #[test_case(Address::new(BYTES); "Address")]
    #[test_case(AssetId::new(BYTES); "AssetId")]
    #[test_case(Nonce::new(BYTES); "Nonce")]
    #[test_case(Vec::from(BYTES); "Vec<u8>")]
    #[test_case(ContractId::new(BYTES); "ContractId")]
    #[test_case(Salt::new(BYTES); "Salt")]
    fn encodes_hex_type_as_machine_readable<T>(value: T)
    where
        HexType: SerializeAs<T>,
    {
        // given
        let expected_bytes = postcard::to_stdvec(&BYTES.as_slice()).unwrap();
        let mut serializer = postcard::Serializer {
            output: AllocVec::default(),
        };

        // when
        <HexType as SerializeAs<T>>::serialize_as(&value, &mut serializer).unwrap();

        // then
        let bytes = serializer.output.finalize().unwrap();
        assert_eq!(bytes, expected_bytes);
    }

    #[test_case(Bytes32::new(BYTES); "Bytes32")]
    #[test_case(Address::new(BYTES); "Address")]
    #[test_case(AssetId::new(BYTES); "AssetId")]
    #[test_case(Nonce::new(BYTES); "Nonce")]
    #[test_case(Vec::from(BYTES); "Vec<u8>")]
    #[test_case(ContractId::new(BYTES); "ContractId")]
    #[test_case(Salt::new(BYTES); "Salt")]
    fn decodes_hex_type_as_machine_readable<T>(value: T)
    where
        for<'a> HexType: DeserializeAs<'a, T>,
        T: PartialEq + std::fmt::Debug,
    {
        // given
        let expected_bytes = postcard::to_stdvec(&BYTES.as_slice()).unwrap();
        let mut serializer = postcard::Deserializer::from_bytes(&expected_bytes);

        // when
        let decoded =
            <HexType as DeserializeAs<T>>::deserialize_as(&mut serializer).unwrap();

        // then
        assert_eq!(value, decoded);
    }
}
