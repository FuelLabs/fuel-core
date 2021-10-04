mod schema {
    cynic::use_schema!("./assets/schema.sdl");
}

use crate::client::schema::ConversionError::{HexString256LengthError, HexStringPrefixError};
use cynic::impl_scalar;
use cynic::serde::{Deserializer, Serialize, Serializer};
use fuel_vm::prelude::{Address, Bytes32, Color, ContractId};
use hex::FromHexError;
use serde::de::Error;
use serde::Deserialize;
use std::convert::TryInto;
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
    pub register: i32,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "RegisterArgs"
)]
pub struct Register {
    #[arguments(id = &args.id, register = &args.register)]
    pub register: i32,
}

#[derive(cynic::FragmentArguments)]
pub struct MemoryArgs {
    pub id: cynic::Id,
    pub start: i32,
    pub size: i32,
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

#[derive(cynic::Scalar, Debug, Clone)]
pub struct HexString256(pub Bytes256);

impl FromStr for HexString256 {
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = Bytes256::from_str(s)?;
        Ok(HexString256(b))
    }
}

#[derive(Debug, Clone)]
pub struct Bytes256(pub [u8; 32]);

impl Serialize for Bytes256 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex = format!("0x{}", hex::encode(self.0));
        serializer.serialize_str(hex.as_str())
    }
}

impl<'de> Deserialize<'de> for Bytes256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: &str = Deserialize::deserialize(deserializer)?;
        Self::from_str(s).map_err(D::Error::custom)
    }
}

impl FromStr for Bytes256 {
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // trim leading 0x
        let value = s.strip_prefix("0x").ok_or(HexStringPrefixError)?;
        // pad input to 32 bytes
        let mut bytes = ((value.len() / 2)..32).map(|_| 0).collect::<Vec<u8>>();
        // decode value into bytes
        bytes.extend(hex::decode(value)?);
        // attempt conversion to fixed length array, error if too long
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|e: Vec<u8>| HexString256LengthError(e.len()))?;
        Ok(Self(bytes))
    }
}

impl From<HexString256> for ContractId {
    fn from(s: HexString256) -> Self {
        ContractId::new(s.0 .0)
    }
}

impl From<HexString256> for Color {
    fn from(s: HexString256) -> Self {
        Color::new(s.0 .0)
    }
}

impl From<HexString256> for Bytes32 {
    fn from(s: HexString256) -> Self {
        Bytes32::new(s.0 .0)
    }
}

impl From<HexString256> for Address {
    fn from(s: HexString256) -> Self {
        Address::new(s.0 .0)
    }
}

#[derive(cynic::Scalar, Debug, Clone)]
pub struct HexString(pub Vec<u8>);

#[derive(cynic::FragmentArguments, Debug)]
pub struct ConnectionArgs {
    pub after: Option<String>,
    pub before: Option<String>,
    pub first: Option<i32>,
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
    #[error("failed integer conversion")]
    IntegerConversionError,
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
        ConversionError::IntegerConversionError
    }
}
