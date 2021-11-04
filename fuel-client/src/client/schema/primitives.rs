use super::schema;
use crate::client::schema::ConversionError;
use crate::client::schema::ConversionError::HexStringPrefixError;
use cynic::impl_scalar;
use fuel_types::{Address, Bytes32, Color, ContractId, Salt};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Debug, Display, Formatter, LowerHex};
use std::str::FromStr;

pub type DateTime = chrono::DateTime<chrono::Utc>;
impl_scalar!(DateTime, schema::DateTime);

#[derive(Debug, Clone, Default)]
pub struct HexFormatted<T: Debug + Clone + Default>(pub T);

impl<T: LowerHex + Debug + Clone + Default> Serialize for HexFormatted<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(format!("{:#x}", self.0).as_str())
    }
}

impl<'de, T: FromStr<Err = E> + Debug + Clone + Default, E: Display> Deserialize<'de>
    for HexFormatted<T>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        T::from_str(s.as_str()).map_err(D::Error::custom).map(Self)
    }
}

impl<T: FromStr<Err = E> + Debug + Clone + Default, E: Display> FromStr for HexFormatted<T> {
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        T::from_str(s)
            .map_err(|e| ConversionError::HexError(format!("{}", e)))
            .map(Self)
    }
}

impl<T: LowerHex + Debug + Clone + Default> Display for HexFormatted<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:x}", self.0)
    }
}

#[derive(cynic::Scalar, Debug, Clone, Default)]
pub struct HexString256(pub HexFormatted<Bytes32>);

impl FromStr for HexString256 {
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = HexFormatted::<Bytes32>::from_str(s)?;
        Ok(HexString256(b))
    }
}

impl From<HexString256> for ContractId {
    fn from(s: HexString256) -> Self {
        ContractId::new(s.0 .0.into())
    }
}

impl From<HexString256> for Color {
    fn from(s: HexString256) -> Self {
        Color::new(s.0 .0.into())
    }
}

impl From<HexString256> for Bytes32 {
    fn from(s: HexString256) -> Self {
        Bytes32::new(s.0 .0.into())
    }
}

impl From<HexString256> for Address {
    fn from(s: HexString256) -> Self {
        Address::new(s.0 .0.into())
    }
}

impl From<HexString256> for Salt {
    fn from(s: HexString256) -> Self {
        Salt::new(s.0 .0.into())
    }
}

#[derive(cynic::Scalar, Debug, Clone)]
pub struct HexString(pub Bytes);

impl From<HexString> for Vec<u8> {
    fn from(s: HexString) -> Self {
        s.0 .0
    }
}

#[derive(Debug, Clone)]
pub struct Bytes(pub Vec<u8>);

impl FromStr for Bytes {
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // trim leading 0x
        let value = s.strip_prefix("0x").ok_or(HexStringPrefixError)?;
        // decode value into bytes
        Ok(Bytes(hex::decode(value)?))
    }
}

impl Serialize for Bytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex = format!("0x{}", hex::encode(&self.0));
        serializer.serialize_str(hex.as_str())
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        Self::from_str(s.as_str()).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, derive_more::Into, derive_more::From, PartialOrd, PartialEq)]
pub struct U64(pub u64);
impl_scalar!(U64, schema::U64);

impl Serialize for U64 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = self.0.to_string();
        serializer.serialize_str(s.as_str())
    }
}

impl<'de> Deserialize<'de> for U64 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        Ok(Self(s.parse().map_err(D::Error::custom)?))
    }
}

impl From<usize> for U64 {
    fn from(i: usize) -> Self {
        U64(i as u64)
    }
}
