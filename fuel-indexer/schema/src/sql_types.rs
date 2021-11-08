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
    Bytes4 = 2,
    Bytes8 = 3,
    Bytes32 = 4,
    Color = 5,
    ContractId = 6,
    Salt = 7,
    Blob = 8,
}

#[cfg(feature = "db-models")]
impl<DB: Backend> ToSql<Columntypename, DB> for ColumnType {
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
        match *self {
            ColumnType::ID => out.write_all(b"ID")?,
            ColumnType::Address => out.write_all(b"Address")?,
            ColumnType::Bytes4 => out.write_all(b"Bytes4")?,
            ColumnType::Bytes8 => out.write_all(b"Bytes8")?,
            ColumnType::Bytes32 => out.write_all(b"Bytes32")?,
            ColumnType::Color => out.write_all(b"Color")?,
            ColumnType::ContractId => out.write_all(b"ContractId")?,
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
            b"Bytes4" => Ok(ColumnType::Bytes4),
            b"Bytes8" => Ok(ColumnType::Bytes8),
            b"Bytes32" => Ok(ColumnType::Bytes32),
            b"Color" => Ok(ColumnType::Color),
            b"ContractId" => Ok(ColumnType::ContractId),
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
            ColumnType::Bytes4 => 2,
            ColumnType::Bytes8 => 3,
            ColumnType::Bytes32 => 4,
            ColumnType::Color => 5,
            ColumnType::ContractId => 6,
            ColumnType::Salt => 7,
            ColumnType::Blob => 8,
        }
    }
}

impl From<i32> for ColumnType {
    fn from(num: i32) -> ColumnType {
        match num {
            0 => ColumnType::ID,
            1 => ColumnType::Address,
            2 => ColumnType::Bytes4,
            3 => ColumnType::Bytes8,
            4 => ColumnType::Bytes32,
            5 => ColumnType::Color,
            6 => ColumnType::ContractId,
            7 => ColumnType::Salt,
            8 => ColumnType::Blob,
            _ => panic!("Invalid column type!"),
        }
    }
}

impl From<&str> for ColumnType {
    fn from(name: &str) -> ColumnType {
        match name {
            "ID" => ColumnType::ID,
            "Address" => ColumnType::Address,
            "Bytes4" => ColumnType::Bytes4,
            "Bytes8" => ColumnType::Bytes8,
            "Bytes32" => ColumnType::Bytes32,
            "Color" => ColumnType::Color,
            "ContractId" => ColumnType::ContractId,
            "Salt" => ColumnType::Salt,
            "Blob`" => ColumnType::Blob,
            _ => panic!("Invalid column type! {}", name),
        }
    }
}
