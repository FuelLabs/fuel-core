use crate::data::value::Object;
use crate::data::value::Word;
use crate::prelude::q;
use crate::prelude::r;
use std::iter::FromIterator;

/// Creates a `graphql_parser::query::Value::Object` from key/value pairs.
/// If you don't need to determine which keys are included dynamically at runtime
/// consider using the `object! {}` macro instead.
pub fn object_value(data: Vec<(&str, r::Value)>) -> r::Value {
    r::Value::Object(Object::from_iter(
        data.into_iter().map(|(k, v)| (Word::from(k), v)),
    ))
}

pub trait IntoValue {
    fn into_value(self) -> r::Value;
}

impl IntoValue for r::Value {
    #[inline]
    fn into_value(self) -> r::Value {
        self
    }
}

impl IntoValue for &'_ str {
    #[inline]
    fn into_value(self) -> r::Value {
        self.to_owned().into_value()
    }
}

impl IntoValue for i32 {
    #[inline]
    fn into_value(self) -> r::Value {
        r::Value::Int(self as i64)
    }
}

impl IntoValue for q::Number {
    #[inline]
    fn into_value(self) -> r::Value {
        r::Value::Int(self.as_i64().unwrap())
    }
}

impl IntoValue for u64 {
    #[inline]
    fn into_value(self) -> r::Value {
        r::Value::String(self.to_string())
    }
}

impl<T: IntoValue> IntoValue for Option<T> {
    #[inline]
    fn into_value(self) -> r::Value {
        match self {
            Some(v) => v.into_value(),
            None => r::Value::Null,
        }
    }
}

impl<T: IntoValue> IntoValue for Vec<T> {
    #[inline]
    fn into_value(self) -> r::Value {
        r::Value::List(self.into_iter().map(|e| e.into_value()).collect::<Vec<_>>())
    }
}

impl IntoValue for &[u8] {
    #[inline]
    fn into_value(self) -> r::Value {
        r::Value::String(format!("0x{}", hex::encode(self)))
    }
}

impl IntoValue for chrono::NaiveDate {
    #[inline]
    fn into_value(self) -> r::Value {
        r::Value::String(self.format("%Y-%m-%d").to_string())
    }
}

macro_rules! impl_into_values {
    ($(($T:ty, $V:ident)),*) => {
        $(
            impl IntoValue for $T {
                #[inline]
                fn into_value(self) -> r::Value {
                    r::Value::$V(self)
                }
            }
        )+
    };
}

impl_into_values![(String, String), (f64, Float), (bool, Boolean)];

/// Creates a `data::value::Value::Object` from key/value pairs.
#[macro_export]
macro_rules! object {
    ($($name:ident: $value:expr,)*) => {
        {
            use $crate::data::value::Word;
            let mut result = Vec::new();
            $(
                let value = $crate::data::graphql::object_macro::IntoValue::into_value($value);
                result.push((Word::from(stringify!($name)), value));
            )*
            $crate::prelude::r::Value::Object($crate::data::value::Object::from_iter(result))
        }
    };
    ($($name:ident: $value:expr),*) => {
        object! {$($name: $value,)*}
    };
}
