#[cfg(any(feature = "db-postgres", feature = "db-sqlite"))]
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql},
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Text,
};

#[cfg(any(feature = "db-postgres", feature = "db-sqlite"))]
use crate::db::DBBackend;

#[cfg(any(feature = "db-postgres", feature = "db-sqlite"))]
use std::io::Write;

#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(any(feature = "db-postgres", feature = "db-sqlite"), derive(AsExpression, FromSqlRow))]
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

impl ColumnType {
    pub fn to_string(self) -> String {
        match self {
            ColumnType::ID => "ID".into(),
            ColumnType::Address => "Address".into(),
            ColumnType::AssetId => "AssetId".into(),
            ColumnType::Bytes4 => "Bytes4".into(),
            ColumnType::Bytes8 => "Bytes8".into(),
            ColumnType::Bytes32 => "Bytes32".into(),
            ColumnType::ContractId => "ContractId".into(),
            ColumnType::Salt => "Salt".into(),
            ColumnType::Int4 => "Int4".into(),
            ColumnType::Int8 => "Int8".into(),
            ColumnType::UInt4 => "UInt4".into(),
            ColumnType::UInt8 => "UInt8".into(),
            ColumnType::Timestamp => "Timestamp".into(),
            ColumnType::Blob => "Blob".into(),
        }
    }
}


#[cfg(any(feature = "db-postgres", feature = "db-sqlite"))]
impl diesel::Expression for ColumnType {
    type SqlType = diesel::sql_types::Text;
}

#[cfg(any(feature = "db-postgres", feature = "db-sqlite"))]
impl<DB: Backend> ToSql<Text, DB> for ColumnType {
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


#[cfg(feature = "db-postgres")]
impl FromSql<diesel::sql_types::Text, DBBackend> for ColumnType {
    fn from_sql(bytes: Option<&<DBBackend as Backend>::RawValue>) -> deserialize::Result<Self> {
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

#[cfg(feature = "db-sqlite")]
impl FromSql<Text, DBBackend> for ColumnType {
    fn from_sql(bytes: Option<&<DBBackend as Backend>::RawValue>) -> deserialize::Result<Self> {
        match not_none!(bytes).read_text() {
            "ID" => Ok(ColumnType::ID),
            "Address" => Ok(ColumnType::Address),
            "AssetId" => Ok(ColumnType::AssetId),
            "Bytes4" => Ok(ColumnType::Bytes4),
            "Bytes8" => Ok(ColumnType::Bytes8),
            "Bytes32" => Ok(ColumnType::Bytes32),
            "ContractId" => Ok(ColumnType::ContractId),
            "Int4" => Ok(ColumnType::Int4),
            "Int8" => Ok(ColumnType::Int8),
            "UInt4" => Ok(ColumnType::UInt4),
            "UInt8" => Ok(ColumnType::UInt8),
            "Timestamp" => Ok(ColumnType::Timestamp),
            "Salt" => Ok(ColumnType::Salt),
            "Blob" => Ok(ColumnType::Blob),
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
            "Blob" => ColumnType::Blob,
            _ => panic!("Invalid column type! {}", name),
        }
    }
}
