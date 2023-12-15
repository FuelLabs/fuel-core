use anyhow::{anyhow, Error};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::str::FromStr;

use crate::blockchain::BlockHash;
use crate::data::value::Object;
use crate::prelude::{r, BigInt};
use web3::types::H160;

pub trait TryFromValue: Sized {
    fn try_from_value(value: &r::Value) -> Result<Self, Error>;
}

impl TryFromValue for r::Value {
    fn try_from_value(value: &r::Value) -> Result<Self, Error> {
        Ok(value.clone())
    }
}

impl TryFromValue for bool {
    fn try_from_value(value: &r::Value) -> Result<Self, Error> {
        match value {
            r::Value::Boolean(b) => Ok(*b),
            _ => Err(anyhow!("Cannot parse value into a boolean: {:?}", value)),
        }
    }
}

impl TryFromValue for String {
    fn try_from_value(value: &r::Value) -> Result<Self, Error> {
        match value {
            r::Value::String(s) => Ok(s.clone()),
            r::Value::Enum(s) => Ok(s.clone()),
            _ => Err(anyhow!("Cannot parse value into a string: {:?}", value)),
        }
    }
}

impl TryFromValue for u64 {
    fn try_from_value(value: &r::Value) -> Result<Self, Error> {
        match value {
            r::Value::Int(n) => {
                if *n >= 0 {
                    Ok(*n as u64)
                } else {
                    Err(anyhow!("Cannot parse value into an integer/u64: {:?}", n))
                }
            }
            // `BigInt`s are represented as `String`s.
            r::Value::String(s) => u64::from_str(s).map_err(Into::into),
            _ => Err(anyhow!(
                "Cannot parse value into an integer/u64: {:?}",
                value
            )),
        }
    }
}

impl TryFromValue for i32 {
    fn try_from_value(value: &r::Value) -> Result<Self, Error> {
        match value {
            r::Value::Int(n) => {
                let n = *n;
                i32::try_from(n).map_err(Error::from)
            }
            // `BigInt`s are represented as `String`s.
            r::Value::String(s) => i32::from_str(s).map_err(Into::into),
            _ => Err(anyhow!(
                "Cannot parse value into an integer/u64: {:?}",
                value
            )),
        }
    }
}

impl TryFromValue for H160 {
    fn try_from_value(value: &r::Value) -> Result<Self, Error> {
        match value {
            r::Value::String(s) => {
                // `H160::from_str` takes a hex string with no leading `0x`.
                let string = s.trim_start_matches("0x");
                H160::from_str(string).map_err(|e| {
                    anyhow!("Cannot parse Address/H160 value from string `{}`: {}", s, e)
                })
            }
            _ => Err(anyhow!(
                "Cannot parse value into an Address/H160: {:?}",
                value
            )),
        }
    }
}

impl TryFromValue for BlockHash {
    fn try_from_value(value: &r::Value) -> Result<Self, Error> {
        match value {
            r::Value::String(s) => BlockHash::from_str(s)
                .map_err(|e| anyhow!("Cannot parse hex value from string `{}`: {}", s, e)),
            _ => Err(anyhow!("Cannot parse non-string value: {:?}", value)),
        }
    }
}

impl TryFromValue for BigInt {
    fn try_from_value(value: &r::Value) -> Result<Self, Error> {
        match value {
            r::Value::String(s) => BigInt::from_str(s)
                .map_err(|e| anyhow!("Cannot parse BigInt value from string `{}`: {}", s, e)),
            _ => Err(anyhow!("Cannot parse value into an BigInt: {:?}", value)),
        }
    }
}

impl<T> TryFromValue for Vec<T>
where
    T: TryFromValue,
{
    fn try_from_value(value: &r::Value) -> Result<Self, Error> {
        match value {
            r::Value::List(values) => values.iter().try_fold(vec![], |mut values, value| {
                values.push(T::try_from_value(value)?);
                Ok(values)
            }),
            _ => Err(anyhow!("Cannot parse value into a vector: {:?}", value)),
        }
    }
}

pub trait ValueMap {
    fn get_required<T: TryFromValue>(&self, key: &str) -> Result<T, Error>;
    fn get_optional<T: TryFromValue>(&self, key: &str) -> Result<Option<T>, Error>;
}

impl ValueMap for r::Value {
    fn get_required<T: TryFromValue>(&self, key: &str) -> Result<T, Error> {
        match self {
            r::Value::Object(map) => map.get_required(key),
            _ => Err(anyhow!("value is not a map: {:?}", self)),
        }
    }

    fn get_optional<T>(&self, key: &str) -> Result<Option<T>, Error>
    where
        T: TryFromValue,
    {
        match self {
            r::Value::Object(map) => map.get_optional(key),
            _ => Err(anyhow!("value is not a map: {:?}", self)),
        }
    }
}

impl ValueMap for &Object {
    fn get_required<T>(&self, key: &str) -> Result<T, Error>
    where
        T: TryFromValue,
    {
        self.get(key)
            .ok_or_else(|| anyhow!("Required field `{}` not set", key))
            .and_then(T::try_from_value)
    }

    fn get_optional<T>(&self, key: &str) -> Result<Option<T>, Error>
    where
        T: TryFromValue,
    {
        self.get(key).map_or(Ok(None), |value| match value {
            r::Value::Null => Ok(None),
            _ => T::try_from_value(value).map(Some),
        })
    }
}

impl ValueMap for &HashMap<&str, r::Value> {
    fn get_required<T>(&self, key: &str) -> Result<T, Error>
    where
        T: TryFromValue,
    {
        self.get(key)
            .ok_or_else(|| anyhow!("Required field `{}` not set", key))
            .and_then(T::try_from_value)
    }

    fn get_optional<T>(&self, key: &str) -> Result<Option<T>, Error>
    where
        T: TryFromValue,
    {
        self.get(key).map_or(Ok(None), |value| match value {
            r::Value::Null => Ok(None),
            _ => T::try_from_value(value).map(Some),
        })
    }
}

pub trait ValueList {
    fn get_values<T>(&self) -> Result<Vec<T>, Error>
    where
        T: TryFromValue;
}

impl ValueList for r::Value {
    fn get_values<T>(&self) -> Result<Vec<T>, Error>
    where
        T: TryFromValue,
    {
        match self {
            r::Value::List(values) => values.get_values(),
            _ => Err(anyhow!("value is not a list: {:?}", self)),
        }
    }
}

impl ValueList for Vec<r::Value> {
    fn get_values<T>(&self) -> Result<Vec<T>, Error>
    where
        T: TryFromValue,
    {
        self.iter().try_fold(vec![], |mut acc, value| {
            acc.push(T::try_from_value(value)?);
            Ok(acc)
        })
    }
}
