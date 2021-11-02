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

#[cfg(feature = "use-std")]
pub const BASE_SCHEMA: &str = include_str!("./base.graphql");

pub mod sql_types;

#[cfg(feature = "db-models")]
pub mod db;

pub use fuel_tx::{Address, Bytes32, Bytes4, Bytes8, Color, ContractId, Salt};

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
        match self {
            FtColumn::ID(value) => {
                format!("{}", value)
            }
            FtColumn::Address(value) => {
                format!("'{:x}'", value)
            }
            FtColumn::Bytes4(value) => {
                format!("'{:x}'", value)
            }
            FtColumn::Bytes8(value) => {
                format!("'{:x}'", value)
            }
            FtColumn::Bytes32(value) => {
                format!("'{:x}'", value)
            }
            FtColumn::Color(value) => {
                format!("'{:x}'", value)
            }
            FtColumn::ContractId(value) => {
                format!("'{:x}'", value)
            }
            FtColumn::Salt(value) => {
                format!("'{:x}'", value)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(feature = "use-std", test)]
    fn test_fragments() {
        let id = FtColumn::ID(123456);
        let addr = FtColumn::Address(Address::try_from([0x12; 32]).expect("Bad bytes"));
        let bytes4 = FtColumn::Bytes4(Bytes4::try_from([0xF0; 4]).expect("Bad bytes"));
        let bytes8 = FtColumn::Bytes8(Bytes8::try_from([0x9D; 8]).expect("Bad bytes"));
        let bytes32 = FtColumn::Bytes32(Bytes32::try_from([0xEE; 32]).expect("Bad bytes"));
        let color = FtColumn::Color(Color::try_from([0xA5; 32]).expect("Bad bytes"));
        let contractid = FtColumn::ContractId(ContractId::try_from([0x78; 32]).expect("Bad bytes"));
        let salt = FtColumn::Salt(Salt::try_from([0x31; 32]).expect("Bad bytes"));

        insta::assert_yaml_snapshot!(id.query_fragment());
        insta::assert_yaml_snapshot!(addr.query_fragment());
        insta::assert_yaml_snapshot!(bytes4.query_fragment());
        insta::assert_yaml_snapshot!(bytes8.query_fragment());
        insta::assert_yaml_snapshot!(bytes32.query_fragment());
        insta::assert_yaml_snapshot!(color.query_fragment());
        insta::assert_yaml_snapshot!(contractid.query_fragment());
        insta::assert_yaml_snapshot!(salt.query_fragment());
    }
}
