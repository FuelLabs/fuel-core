pub mod schema {
    cynic::use_schema!("./assets/schema.sdl");
}

use crate::client::schema::ConversionError::HexStringPrefixError;
use cynic::impl_scalar;
use cynic::serde::{Deserializer, Serialize, Serializer};
use fuel_tx::Salt;
use fuel_vm::prelude::{Address, Bytes32, Color, ContractId};
use hex::FromHexError;
use serde::de::Error;
use serde::Deserialize;
use std::fmt::{Debug, Display, Formatter, LowerHex};
use std::io::ErrorKind;
use std::num::TryFromIntError;
use std::str::FromStr;
use thiserror::Error;

type DateTime = chrono::DateTime<chrono::Utc>;
impl_scalar!(DateTime, schema::DateTime);

pub mod block;
pub mod chain;
pub mod coin;
pub mod tx;

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Query")]
pub struct Health {
    pub health: bool,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Mutation")]
pub struct StartSession {
    pub start_session: cynic::Id,
}

#[derive(cynic::FragmentArguments)]
pub struct IdArg {
    pub id: cynic::Id,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Mutation",
    argument_struct = "IdArg"
)]
pub struct EndSession {
    #[arguments(id = &args.id)]
    pub end_session: bool,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Mutation",
    argument_struct = "IdArg"
)]
pub struct Reset {
    #[arguments(id = &args.id)]
    pub reset: bool,
}

#[derive(cynic::FragmentArguments)]
pub struct ExecuteArgs {
    pub id: cynic::Id,
    pub op: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Mutation",
    argument_struct = "ExecuteArgs"
)]
pub struct Execute {
    #[arguments(id = &args.id, op = &args.op)]
    pub execute: bool,
}

#[derive(cynic::FragmentArguments)]
pub struct RegisterArgs {
    pub id: cynic::Id,
    pub register: U64,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "RegisterArgs"
)]
pub struct Register {
    #[arguments(id = &args.id, register = &args.register)]
    pub register: U64,
}

#[derive(cynic::FragmentArguments)]
pub struct MemoryArgs {
    pub id: cynic::Id,
    pub start: U64,
    pub size: U64,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "MemoryArgs"
)]
pub struct Memory {
    #[arguments(id = &args.id, start = &args.start, size = &args.size)]
    pub memory: String,
}

#[derive(Debug, Clone, Default)]
pub struct HexFormatted<T: Debug + Clone + Default>(pub T);

impl<T: LowerHex + Debug + Clone + Default> Serialize for HexFormatted<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(format!("0x{:x}", self.0).as_str())
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

/// Generic graphql pagination query args
#[derive(cynic::FragmentArguments, Debug)]
pub struct ConnectionArgs {
    /// Skip until cursor (forward pagination)
    pub after: Option<String>,
    /// Skip until cursor (backward pagination)
    pub before: Option<String>,
    /// Retrieve the first n items in order (forward pagination)
    pub first: Option<i32>,
    /// Retrieve the last n items in order (backward pagination).
    /// Can't be used at the same time as `first`.
    pub last: Option<i32>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct PageInfo {
    pub end_cursor: Option<String>,
    pub has_next_page: bool,
    pub has_previous_page: bool,
    pub start_cursor: Option<String>,
}

#[derive(Error, Debug)]
pub enum ConversionError {
    #[error("Field is required from the GraphQL response {0}")]
    MissingField(String),
    #[error("expected 0x prefix")]
    HexStringPrefixError,
    #[error("expected 32 bytes, received {0}")]
    HexString256LengthError(usize),
    #[error("hex parsing error {0}")]
    HexDecodingError(FromHexError),
    #[error("hex parsing error {0}")]
    HexError(String),
    #[error("failed integer conversion")]
    IntegerConversion,
    #[error("failed to deserialize transaction from bytes {0}")]
    TransactionFromBytesError(std::io::Error),
    #[error("failed to deserialize receipt from bytes {0}")]
    ReceiptFromBytesError(std::io::Error),
}

impl From<FromHexError> for ConversionError {
    fn from(hex_error: FromHexError) -> Self {
        Self::HexDecodingError(hex_error)
    }
}

impl From<ConversionError> for std::io::Error {
    fn from(e: ConversionError) -> Self {
        std::io::Error::new(ErrorKind::Other, e)
    }
}

impl From<TryFromIntError> for ConversionError {
    fn from(_: TryFromIntError) -> Self {
        ConversionError::IntegerConversion
    }
}
