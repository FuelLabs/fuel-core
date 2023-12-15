use anyhow::anyhow;
use diesel::pg::Pg;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::{Binary, Bool, Int8, Integer, Text};

use std::io::Write;
use std::str::FromStr;

use super::{scalar, Value};

impl ToSql<Bool, Pg> for Value {
    fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> serialize::Result {
        match self {
            Value::Bool(b) => <bool as ToSql<Bool, Pg>>::to_sql(b, out),
            v => Err(anyhow!(
                "Failed to convert non-boolean attribute value to boolean in SQL: {}",
                v
            )
            .into()),
        }
    }
}

impl ToSql<Integer, Pg> for Value {
    fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> serialize::Result {
        match self {
            Value::Int(i) => <i32 as ToSql<Integer, Pg>>::to_sql(i, out),
            v => Err(anyhow!(
                "Failed to convert non-int attribute value to int in SQL: {}",
                v
            )
            .into()),
        }
    }
}

impl ToSql<Int8, Pg> for Value {
    fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> serialize::Result {
        match self {
            Value::Int8(i) => <i64 as ToSql<Int8, Pg>>::to_sql(i, out),
            Value::Int(i) => <i64 as ToSql<Int8, Pg>>::to_sql(&(*i as i64), out),
            v => Err(anyhow!(
                "Failed to convert non-int8 attribute value to int8 in SQL: {}",
                v
            )
            .into()),
        }
    }
}

impl ToSql<Text, Pg> for Value {
    fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> serialize::Result {
        match self {
            Value::String(s) => <String as ToSql<Text, Pg>>::to_sql(s, out),
            Value::Bytes(h) => <String as ToSql<Text, Pg>>::to_sql(&h.to_string(), out),
            v => Err(anyhow!(
                "Failed to convert attribute value to String or Bytes in SQL: {}",
                v
            )
            .into()),
        }
    }
}

impl ToSql<Binary, Pg> for Value {
    fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> serialize::Result {
        match self {
            Value::Bytes(h) => <_ as ToSql<Binary, Pg>>::to_sql(&h.as_slice(), out),
            Value::String(s) => {
                <_ as ToSql<Binary, Pg>>::to_sql(scalar::Bytes::from_str(s)?.as_slice(), out)
            }
            v => Err(anyhow!("Failed to convert attribute value to Bytes in SQL: {}", v).into()),
        }
    }
}
