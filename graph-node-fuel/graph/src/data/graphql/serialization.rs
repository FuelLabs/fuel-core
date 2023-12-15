use crate::prelude::q::*;
use serde::ser::*;

/// Serializable wrapper around a GraphQL value.
pub struct SerializableValue<'a>(pub &'a Value);

impl<'a> Serialize for SerializableValue<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            Value::Boolean(v) => serializer.serialize_bool(*v),
            Value::Enum(v) => serializer.serialize_str(v),
            Value::Float(v) => serializer.serialize_f64(*v),
            Value::Int(v) => serializer.serialize_newtype_struct("Number", &v.as_i64().unwrap()),
            Value::List(l) => {
                let mut seq = serializer.serialize_seq(Some(l.len()))?;
                for v in l {
                    seq.serialize_element(&SerializableValue(v))?;
                }
                seq.end()
            }
            Value::Null => serializer.serialize_none(),
            Value::String(s) => serializer.serialize_str(s),
            Value::Object(o) => {
                let mut map = serializer.serialize_map(Some(o.len()))?;
                for (k, v) in o {
                    map.serialize_entry(k, &SerializableValue(v))?;
                }
                map.end()
            }
            Value::Variable(_) => unreachable!("output cannot contain variables"),
        }
    }
}
