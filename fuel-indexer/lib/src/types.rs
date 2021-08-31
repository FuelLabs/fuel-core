use core::convert::{TryFrom, TryInto};

pub use fuel_tx::{Address, Bytes32, Color, ContractId, Salt};
use serde::{Deserialize, Serialize};
use serde_scale;

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

#[derive(Debug, PartialEq)]
pub enum ColumnType {
    ID = 0,
    Address = 1,
    Bytes32 = 2,
}

impl From<ColumnType> for u32 {
    fn from(typ: ColumnType) -> u32 {
        match typ {
            ColumnType::ID => 0,
            ColumnType::Address => 1,
            ColumnType::Bytes32 => 2,
        }
    }
}

impl From<u32> for ColumnType {
    fn from(num: u32) -> ColumnType {
        match num {
            0 => ColumnType::ID,
            1 => ColumnType::Address,
            2 => ColumnType::Bytes32,
            _ => panic!("Invalid column type!"),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum FtColumn {
    ID(u64),
    Address(Address),
    Bytes32(Bytes32),
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
            ColumnType::Bytes32 => {
                let bytes = Bytes32::try_from(&bytes[..size]).expect("Invalid slice length");
                FtColumn::Bytes32(bytes)
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
                // TODO: should make fuel-tx a no-std feature flag and a LowerHex implementation
                format!(
                    "'0x{}'",
                    (*value)
                        .iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<Vec<String>>()
                        .join("")
                )
            }
            FtColumn::Bytes32(value) => {
                // TODO: should make fuel-tx a no-std feature flag and a LowerHex implementation
                format!(
                    "'0x{}'",
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
