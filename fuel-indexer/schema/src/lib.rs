#![cfg_attr(not(feature = "use-std"), no_std)]
#[cfg(feature = "db-models")]
#[macro_use]
extern crate diesel;
extern crate alloc;
use alloc::vec::Vec;

use core::convert::{TryFrom, TryInto};
use serde::{Deserialize, Serialize};
// serde_scale for now, can look at other options if necessary.
use crate::sql_types::ColumnType;
use serde_scale;

#[cfg(feature = "use-std")]
use sha2::{Digest, Sha256};

pub mod sql_types;

#[cfg(feature = "db-models")]
pub mod schema;

#[cfg(feature = "db-models")]
pub mod models;

pub use fuel_tx::{Address, Bytes32, Bytes4, Bytes8, Color, ContractId, Salt};

/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////
#[derive(Serialize, Deserialize)]
pub struct SomeEvent {
    pub id: ID,
    pub account: Address,
}

#[derive(Serialize, Deserialize)]
pub struct AnotherEvent {
    pub id: ID,
    pub hash: Bytes32,
    pub sub_event: SomeEvent,
}
/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////
/////////////////////////////////////////////////////////////////////

pub type ID = u64;

pub fn serialize(obj: &impl Serialize) -> Vec<u8> {
    serde_scale::to_vec(obj).expect("Serialize failed")
}

pub fn deserialize<'a, T: Deserialize<'a>>(vec: &'a Vec<u8>) -> T {
    serde_scale::from_slice(&vec).expect("Deserialize failed")
}

#[cfg(feature = "use-std")]
pub fn type_id(type_name: &str) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&Sha256::digest(type_name.as_bytes())[..8]);
    u64::from_le_bytes(bytes)
}

#[cfg(feature = "use-std")]
pub fn schema_version(schema: &str) -> String {
    format!("{:x}", Sha256::digest(schema.as_bytes()))
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum FtColumn {
    ID(u64),
    Address(Address),
    Bytes4(Bytes4),
    Bytes8(Bytes8),
    Bytes32(Bytes32),
    Color(Color),
    ContractId(ContractId),
    Salt(Salt),
}

impl FtColumn {
    pub fn new(ty: ColumnType, size: usize, bytes: &[u8]) -> FtColumn {
        match ty {
            ColumnType::ID => {
                let ident =
                    u64::from_le_bytes(bytes[..size].try_into().expect("Invalid slice length"));
                FtColumn::ID(ident)
            }
            ColumnType::Address => {
                let address = Address::try_from(&bytes[..size]).expect("Invalid slice length");
                FtColumn::Address(address)
            }
            ColumnType::Bytes4 => {
                let bytes = Bytes4::try_from(&bytes[..size]).expect("Invalid slice length");
                FtColumn::Bytes4(bytes)
            }
            ColumnType::Bytes8 => {
                let bytes = Bytes8::try_from(&bytes[..size]).expect("Invalid slice length");
                FtColumn::Bytes8(bytes)
            }
            ColumnType::Bytes32 => {
                let bytes = Bytes32::try_from(&bytes[..size]).expect("Invalid slice length");
                FtColumn::Bytes32(bytes)
            }
            ColumnType::Color => {
                let color = Color::try_from(&bytes[..size]).expect("Invalid slice length");
                FtColumn::Color(color)
            }
            ColumnType::ContractId => {
                let contract_id =
                    ContractId::try_from(&bytes[..size]).expect("Invalid slice length");
                FtColumn::ContractId(contract_id)
            }
            ColumnType::Salt => {
                let salt = Salt::try_from(&bytes[..size]).expect("Invalid slice length");
                FtColumn::Salt(salt)
            }
            ColumnType::Blob => {
                panic!("Blob not supported for FtColumn!");
            }
        }
    }

    #[cfg(feature = "use-std")]
    pub fn query_fragment(&self) -> String {
        // TODO: should make fuel-tx a no-std feature flag and a LowerHex implementation
        match self {
            FtColumn::ID(value) => {
                format!("{}", value)
            }
            FtColumn::Address(value) => {
                format!(
                    "'{}'",
                    (*value)
                        .iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<Vec<String>>()
                        .join("")
                )
            }
            FtColumn::Bytes4(value) => {
                format!(
                    "'{}'",
                    (*value)
                        .iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<Vec<String>>()
                        .join("")
                )
            }
            FtColumn::Bytes8(value) => {
                format!(
                    "'{}'",
                    (*value)
                        .iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<Vec<String>>()
                        .join("")
                )
            }
            FtColumn::Bytes32(value) => {
                format!(
                    "'{}'",
                    (*value)
                        .iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<Vec<String>>()
                        .join("")
                )
            }
            FtColumn::Color(value) => {
                format!(
                    "'{}'",
                    (*value)
                        .iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<Vec<String>>()
                        .join("")
                )
            }
            FtColumn::ContractId(value) => {
                format!(
                    "'{}'",
                    (*value)
                        .iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<Vec<String>>()
                        .join("")
                )
            }
            FtColumn::Salt(value) => {
                format!(
                    "'{}'",
                    (*value)
                        .iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<Vec<String>>()
                        .join("")
                )
            }
        }
    }
}
