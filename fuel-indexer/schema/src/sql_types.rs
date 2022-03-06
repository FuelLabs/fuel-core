#[cfg(feature = "db-models")]
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql},
    pg::Pg,
    serialize::{self, IsNull, Output, ToSql},
};

#[cfg(feature = "db-models")]
use std::io::Write;

#[cfg(feature = "db-models")]
#[derive(SqlType)]
#[postgres(type_name = "Columntypename")]
pub struct Columntypename;

#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "db-models", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "db-models", sql_type = "Columntypename")]
pub enum ColumnType {
    ID = 0,
    Address = 1,
    AssetId = 2,
    Bytes4 = 3,
    Bytes8 = 4,
    Bytes32 = 5,
    ContractId = 6,
    Salt = 7,
    Int4 = 8,
    Int8 = 9,
    UInt4 = 10,
    UInt8 = 11,
    Timestamp = 12,
    Blob = 13,
}

#[cfg(feature = "db-models")]
impl<DB: Backend> ToSql<Columntypename, DB> for ColumnType {
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
        match *self {
            ColumnType::ID => out.write_all(b"ID")?,
            ColumnType::Address => out.write_all(b"Address")?,
            ColumnType::AssetId => out.write_all(b"AssetId")?,
            ColumnType::Bytes4 => out.write_all(b"Bytes4")?,
            ColumnType::Bytes8 => out.write_all(b"Bytes8")?,
            ColumnType::Bytes32 => out.write_all(b"Bytes32")?,
            ColumnType::ContractId => out.write_all(b"ContractId")?,
            ColumnType::Int4 => out.write_all(b"Int4")?,
            ColumnType::Int8 => out.write_all(b"Int8")?,
            ColumnType::UInt4 => out.write_all(b"UInt4")?,
            ColumnType::UInt8 => out.write_all(b"UInt8")?,
            ColumnType::Timestamp => out.write_all(b"Timestamp")?,
            ColumnType::Salt => out.write_all(b"Salt")?,
            ColumnType::Blob => out.write_all(b"Blob")?,
        }
        Ok(IsNull::No)
    }
}

#[cfg(feature = "db-models")]
impl FromSql<Columntypename, Pg> for ColumnType {
    fn from_sql(bytes: Option<&<Pg as Backend>::RawValue>) -> deserialize::Result<Self> {
        match not_none!(bytes) {
            b"ID" => Ok(ColumnType::ID),
            b"Address" => Ok(ColumnType::Address),
            b"AssetId" => Ok(ColumnType::AssetId),
            b"Bytes4" => Ok(ColumnType::Bytes4),
            b"Bytes8" => Ok(ColumnType::Bytes8),
            b"Bytes32" => Ok(ColumnType::Bytes32),
            b"ContractId" => Ok(ColumnType::ContractId),
            b"Int4" => Ok(ColumnType::Int4),
            b"Int8" => Ok(ColumnType::Int8),
            b"UInt4" => Ok(ColumnType::UInt4),
            b"UInt8" => Ok(ColumnType::UInt8),
            b"Timestamp" => Ok(ColumnType::Timestamp),
            b"Salt" => Ok(ColumnType::Salt),
            b"Blob" => Ok(ColumnType::Blob),
            _ => Err("Unrecognized enum variant".into()),
        }
    }
}

impl From<ColumnType> for i32 {
    fn from(typ: ColumnType) -> i32 {
        match typ {
            ColumnType::ID => 0,
            ColumnType::Address => 1,
            ColumnType::AssetId => 2,
            ColumnType::Bytes4 => 3,
            ColumnType::Bytes8 => 4,
            ColumnType::Bytes32 => 5,
            ColumnType::ContractId => 6,
            ColumnType::Salt => 7,
            ColumnType::Int4 => 8,
            ColumnType::Int8 => 9,
            ColumnType::UInt4 => 10,
            ColumnType::UInt8 => 11,
            ColumnType::Timestamp => 12,
            ColumnType::Blob => 13,
        }
    }
}

impl From<i32> for ColumnType {
    fn from(num: i32) -> ColumnType {
        match num {
            0 => ColumnType::ID,
            1 => ColumnType::Address,
            2 => ColumnType::AssetId,
            3 => ColumnType::Bytes4,
            4 => ColumnType::Bytes8,
            5 => ColumnType::Bytes32,
            6 => ColumnType::ContractId,
            7 => ColumnType::Salt,
            8 => ColumnType::Int4,
            9 => ColumnType::Int8,
            10 => ColumnType::UInt4,
            11 => ColumnType::UInt8,
            12 => ColumnType::Timestamp,
            13 => ColumnType::Blob,
            _ => panic!("Invalid column type!"),
        }
    }
}

impl From<&str> for ColumnType {
    fn from(name: &str) -> ColumnType {
        match name {
            "ID" => ColumnType::ID,
            "Address" => ColumnType::Address,
            "AssetId" => ColumnType::AssetId,
            "Bytes4" => ColumnType::Bytes4,
            "Bytes8" => ColumnType::Bytes8,
            "Bytes32" => ColumnType::Bytes32,
            "ContractId" => ColumnType::ContractId,
            "Salt" => ColumnType::Salt,
            "Int4" => ColumnType::Int4,
            "Int8" => ColumnType::Int8,
            "UInt4" => ColumnType::UInt4,
            "UInt8" => ColumnType::UInt8,
            "Timestamp" => ColumnType::Timestamp,
            "Blob`" => ColumnType::Blob,
            _ => panic!("Invalid column type! {}", name),
        }
    }
}
