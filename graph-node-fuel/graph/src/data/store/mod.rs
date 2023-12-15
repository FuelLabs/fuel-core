use crate::{
    components::store::DeploymentLocator,
    prelude::{lazy_static, q, r, s, CacheWeight, QueryExecutionError},
    runtime::gas::{Gas, GasSizeOf},
    schema::{EntityKey, EntityType},
    util::intern::{self, AtomPool},
    util::intern::{Error as InternError, NullValue, Object},
};
use crate::{data::subgraph::DeploymentHash, prelude::EntityChange};
use anyhow::{anyhow, Error};
use itertools::Itertools;
use serde::de;
use serde::{Deserialize, Serialize};
use stable_hash::{FieldAddress, StableHash, StableHasher};
use std::borrow::Cow;
use std::convert::TryFrom;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use strum_macros::IntoStaticStr;
use thiserror::Error;

use super::{graphql::TypeExt as _, value::Word};

/// Handling of entity ids
mod id;
pub use id::{Id, IdList, IdRef, IdType};

/// Custom scalars in GraphQL.
pub mod scalar;

// Ethereum compatibility.
pub mod ethereum;

/// Conversion of values to/from SQL
pub mod sql;

/// Filter subscriptions
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SubscriptionFilter {
    /// Receive updates about all entities from the given deployment of the
    /// given type
    Entities(DeploymentHash, EntityType),
    /// Subscripe to changes in deployment assignments
    Assignment,
}

impl SubscriptionFilter {
    pub fn matches(&self, change: &EntityChange) -> bool {
        match (self, change) {
            (
                Self::Entities(eid, etype),
                EntityChange::Data {
                    subgraph_id,
                    entity_type,
                    ..
                },
            ) => subgraph_id == eid && entity_type == etype.as_str(),
            (Self::Assignment, EntityChange::Assignment { .. }) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(s: impl Into<String>) -> Result<Self, ()> {
        let s = s.into();

        // Enforce minimum and maximum length limit
        if s.len() > 63 || s.is_empty() {
            return Err(());
        }

        Ok(NodeId(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl slog::Value for NodeId {
    fn serialize(
        &self,
        _record: &slog::Record,
        key: slog::Key,
        serializer: &mut dyn slog::Serializer,
    ) -> slog::Result {
        serializer.emit_str(key, self.0.as_str())
    }
}

impl<'de> de::Deserialize<'de> for NodeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s: String = de::Deserialize::deserialize(deserializer)?;
        NodeId::new(s.clone())
            .map_err(|()| de::Error::invalid_value(de::Unexpected::Str(&s), &"valid node ID"))
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum AssignmentEvent {
    Add {
        deployment: DeploymentLocator,
        node_id: NodeId,
    },
    Remove {
        deployment: DeploymentLocator,
        node_id: NodeId,
    },
}

impl AssignmentEvent {
    pub fn node_id(&self) -> &NodeId {
        match self {
            AssignmentEvent::Add { node_id, .. } => node_id,
            AssignmentEvent::Remove { node_id, .. } => node_id,
        }
    }
}

/// An entity attribute name is represented as a string.
pub type Attribute = String;

pub const BYTES_SCALAR: &str = "Bytes";
pub const BIG_INT_SCALAR: &str = "BigInt";
pub const BIG_DECIMAL_SCALAR: &str = "BigDecimal";
pub const INT8_SCALAR: &str = "Int8";

#[derive(Clone, Debug, PartialEq)]
pub enum ValueType {
    Boolean,
    BigInt,
    Bytes,
    BigDecimal,
    Int,
    Int8,
    String,
}

impl FromStr for ValueType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Boolean" => Ok(ValueType::Boolean),
            "BigInt" => Ok(ValueType::BigInt),
            "Bytes" => Ok(ValueType::Bytes),
            "BigDecimal" => Ok(ValueType::BigDecimal),
            "Int" => Ok(ValueType::Int),
            "Int8" => Ok(ValueType::Int8),
            "String" | "ID" => Ok(ValueType::String),
            s => Err(anyhow!("Type not available in this context: {}", s)),
        }
    }
}

impl ValueType {
    /// Return `true` if `s` is the name of a builtin scalar type
    pub fn is_scalar(s: &str) -> bool {
        Self::from_str(s).is_ok()
    }
}

// Note: Do not modify fields without also making a backward compatible change to the StableHash impl (below)
/// An attribute value is represented as an enum with variants for all supported value types.
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
#[derive(IntoStaticStr)]
pub enum Value {
    String(String),
    Int(i32),
    Int8(i64),
    BigDecimal(scalar::BigDecimal),
    Bool(bool),
    List(Vec<Value>),
    Null,
    Bytes(scalar::Bytes),
    BigInt(scalar::BigInt),
}

pub const NULL: Value = Value::Null;

impl stable_hash_legacy::StableHash for Value {
    fn stable_hash<H: stable_hash_legacy::StableHasher>(
        &self,
        mut sequence_number: H::Seq,
        state: &mut H,
    ) {
        use stable_hash_legacy::prelude::*;
        use Value::*;

        // This is the default, so write nothing.
        if self == &Null {
            return;
        }
        stable_hash_legacy::StableHash::stable_hash(
            &Into::<&str>::into(self).to_string(),
            sequence_number.next_child(),
            state,
        );

        match self {
            Null => unreachable!(),
            String(inner) => {
                stable_hash_legacy::StableHash::stable_hash(inner, sequence_number, state)
            }
            Int(inner) => {
                stable_hash_legacy::StableHash::stable_hash(inner, sequence_number, state)
            }
            Int8(inner) => {
                stable_hash_legacy::StableHash::stable_hash(inner, sequence_number, state)
            }
            BigDecimal(inner) => {
                stable_hash_legacy::StableHash::stable_hash(inner, sequence_number, state)
            }
            Bool(inner) => {
                stable_hash_legacy::StableHash::stable_hash(inner, sequence_number, state)
            }
            List(inner) => {
                stable_hash_legacy::StableHash::stable_hash(inner, sequence_number, state)
            }
            Bytes(inner) => {
                stable_hash_legacy::StableHash::stable_hash(inner, sequence_number, state)
            }
            BigInt(inner) => {
                stable_hash_legacy::StableHash::stable_hash(inner, sequence_number, state)
            }
        }
    }
}

impl StableHash for Value {
    fn stable_hash<H: StableHasher>(&self, field_address: H::Addr, state: &mut H) {
        use Value::*;

        // This is the default, so write nothing.
        if self == &Null {
            return;
        }

        let variant = match self {
            Null => unreachable!(),
            String(inner) => {
                inner.stable_hash(field_address.child(0), state);
                1
            }
            Int(inner) => {
                inner.stable_hash(field_address.child(0), state);
                2
            }
            BigDecimal(inner) => {
                inner.stable_hash(field_address.child(0), state);
                3
            }
            Bool(inner) => {
                inner.stable_hash(field_address.child(0), state);
                4
            }
            List(inner) => {
                inner.stable_hash(field_address.child(0), state);
                5
            }
            Bytes(inner) => {
                inner.stable_hash(field_address.child(0), state);
                6
            }
            BigInt(inner) => {
                inner.stable_hash(field_address.child(0), state);
                7
            }
            Int8(inner) => {
                inner.stable_hash(field_address.child(0), state);
                8
            }
        };

        state.write(field_address, &[variant])
    }
}

impl NullValue for Value {
    fn null() -> Self {
        Value::Null
    }
}

impl Value {
    pub fn from_query_value(value: &r::Value, ty: &s::Type) -> Result<Value, QueryExecutionError> {
        use graphql_parser::schema::Type::{ListType, NamedType, NonNullType};

        Ok(match (value, ty) {
            // When dealing with non-null types, use the inner type to convert the value
            (value, NonNullType(t)) => Value::from_query_value(value, t)?,

            (r::Value::List(values), ListType(ty)) => Value::List(
                values
                    .iter()
                    .map(|value| Self::from_query_value(value, ty))
                    .collect::<Result<Vec<_>, _>>()?,
            ),

            (r::Value::List(values), NamedType(n)) => Value::List(
                values
                    .iter()
                    .map(|value| Self::from_query_value(value, &NamedType(n.to_string())))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            (r::Value::Enum(e), NamedType(_)) => Value::String(e.clone()),
            (r::Value::String(s), NamedType(n)) => {
                // Check if `ty` is a custom scalar type, otherwise assume it's
                // just a string.
                match n.as_str() {
                    BYTES_SCALAR => Value::Bytes(scalar::Bytes::from_str(s)?),
                    BIG_INT_SCALAR => Value::BigInt(scalar::BigInt::from_str(s).map_err(|e| {
                        QueryExecutionError::ValueParseError("BigInt".to_string(), format!("{}", e))
                    })?),
                    BIG_DECIMAL_SCALAR => Value::BigDecimal(scalar::BigDecimal::from_str(s)?),
                    INT8_SCALAR => Value::Int8(s.parse::<i64>().map_err(|_| {
                        QueryExecutionError::ValueParseError("Int8".to_string(), format!("{}", s))
                    })?),
                    _ => Value::String(s.clone()),
                }
            }
            (r::Value::Int(i), _) => Value::Int(*i as i32),
            (r::Value::Boolean(b), _) => Value::Bool(b.to_owned()),
            (r::Value::Null, _) => Value::Null,
            _ => {
                return Err(QueryExecutionError::AttributeTypeError(
                    value.to_string(),
                    ty.to_string(),
                ));
            }
        })
    }

    pub fn as_string(self) -> Option<String> {
        if let Value::String(s) = self {
            Some(s)
        } else {
            None
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        if let Value::String(s) = self {
            Some(s.as_str())
        } else {
            None
        }
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }

    pub fn as_int(&self) -> Option<i32> {
        if let Value::Int(i) = self {
            Some(*i)
        } else {
            None
        }
    }

    pub fn as_big_decimal(self) -> Option<scalar::BigDecimal> {
        if let Value::BigDecimal(d) = self {
            Some(d)
        } else {
            None
        }
    }

    pub fn as_bool(self) -> Option<bool> {
        if let Value::Bool(b) = self {
            Some(b)
        } else {
            None
        }
    }

    pub fn as_list(self) -> Option<Vec<Value>> {
        if let Value::List(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_bytes(self) -> Option<scalar::Bytes> {
        if let Value::Bytes(b) = self {
            Some(b)
        } else {
            None
        }
    }

    pub fn as_bigint(self) -> Option<scalar::BigInt> {
        if let Value::BigInt(b) = self {
            Some(b)
        } else {
            None
        }
    }

    /// Return the name of the type of this value for display to the user
    pub fn type_name(&self) -> String {
        match self {
            Value::BigDecimal(_) => "BigDecimal".to_owned(),
            Value::BigInt(_) => "BigInt".to_owned(),
            Value::Bool(_) => "Boolean".to_owned(),
            Value::Bytes(_) => "Bytes".to_owned(),
            Value::Int(_) => "Int".to_owned(),
            Value::Int8(_) => "Int8".to_owned(),
            Value::List(values) => {
                if let Some(v) = values.first() {
                    format!("[{}]", v.type_name())
                } else {
                    "[Any]".to_owned()
                }
            }
            Value::Null => "Null".to_owned(),
            Value::String(_) => "String".to_owned(),
        }
    }

    pub fn is_assignable(&self, scalar_type: &ValueType, is_list: bool) -> bool {
        match (self, scalar_type) {
            (Value::String(_), ValueType::String)
            | (Value::BigDecimal(_), ValueType::BigDecimal)
            | (Value::BigInt(_), ValueType::BigInt)
            | (Value::Bool(_), ValueType::Boolean)
            | (Value::Bytes(_), ValueType::Bytes)
            | (Value::Int(_), ValueType::Int)
            | (Value::Int8(_), ValueType::Int8)
            | (Value::Null, _) => true,
            (Value::List(values), _) if is_list => values
                .iter()
                .all(|value| value.is_assignable(scalar_type, false)),
            _ => false,
        }
    }

    fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Value::String(s) => s.to_string(),
                Value::Int(i) => i.to_string(),
                Value::Int8(i) => i.to_string(),
                Value::BigDecimal(d) => d.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => "null".to_string(),
                Value::List(ref values) =>
                    format!("[{}]", values.iter().map(ToString::to_string).join(", ")),
                Value::Bytes(ref bytes) => bytes.to_string(),
                Value::BigInt(ref number) => number.to_string(),
            }
        )
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => f.debug_tuple("String").field(s).finish(),
            Self::Int(i) => f.debug_tuple("Int").field(i).finish(),
            Self::Int8(i) => f.debug_tuple("Int8").field(i).finish(),
            Self::BigDecimal(d) => d.fmt(f),
            Self::Bool(arg0) => f.debug_tuple("Bool").field(arg0).finish(),
            Self::List(arg0) => f.debug_tuple("List").field(arg0).finish(),
            Self::Null => write!(f, "Null"),
            Self::Bytes(bytes) => bytes.fmt(f),
            Self::BigInt(number) => number.fmt(f),
        }
    }
}

impl From<Value> for q::Value {
    fn from(value: Value) -> Self {
        match value {
            Value::String(s) => q::Value::String(s),
            Value::Int(i) => q::Value::Int(q::Number::from(i)),
            Value::Int8(i) => q::Value::String(i.to_string()),
            Value::BigDecimal(d) => q::Value::String(d.to_string()),
            Value::Bool(b) => q::Value::Boolean(b),
            Value::Null => q::Value::Null,
            Value::List(values) => {
                q::Value::List(values.into_iter().map(|value| value.into()).collect())
            }
            Value::Bytes(bytes) => q::Value::String(bytes.to_string()),
            Value::BigInt(number) => q::Value::String(number.to_string()),
        }
    }
}

impl From<Value> for r::Value {
    fn from(value: Value) -> Self {
        match value {
            Value::String(s) => r::Value::String(s),
            Value::Int(i) => r::Value::Int(i as i64),
            Value::Int8(i) => r::Value::String(i.to_string()),
            Value::BigDecimal(d) => r::Value::String(d.to_string()),
            Value::Bool(b) => r::Value::Boolean(b),
            Value::Null => r::Value::Null,
            Value::List(values) => {
                r::Value::List(values.into_iter().map(|value| value.into()).collect())
            }
            Value::Bytes(bytes) => r::Value::String(bytes.to_string()),
            Value::BigInt(number) => r::Value::String(number.to_string()),
        }
    }
}

impl<'a> From<&'a str> for Value {
    fn from(value: &'a str) -> Value {
        Value::String(value.to_owned())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Value {
        Value::String(value)
    }
}

impl<'a> From<&'a String> for Value {
    fn from(value: &'a String) -> Value {
        Value::String(value.clone())
    }
}

impl From<scalar::Bytes> for Value {
    fn from(value: scalar::Bytes) -> Value {
        Value::Bytes(value)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Value {
        Value::Bool(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Value {
        Value::Int(value)
    }
}

impl From<scalar::BigDecimal> for Value {
    fn from(value: scalar::BigDecimal) -> Value {
        Value::BigDecimal(value)
    }
}

impl From<scalar::BigInt> for Value {
    fn from(value: scalar::BigInt) -> Value {
        Value::BigInt(value)
    }
}

impl From<u64> for Value {
    fn from(value: u64) -> Value {
        Value::BigInt(value.into())
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Value {
        Value::Int8(value.into())
    }
}

impl TryFrom<Value> for Option<scalar::BigInt> {
    type Error = Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::BigInt(n) => Ok(Some(n)),
            Value::Null => Ok(None),
            _ => Err(anyhow!("Value is not an BigInt")),
        }
    }
}

impl<T> From<Vec<T>> for Value
where
    T: Into<Value>,
{
    fn from(values: Vec<T>) -> Value {
        Value::List(values.into_iter().map(Into::into).collect())
    }
}

impl<T> From<Option<T>> for Value
where
    Value: From<T>,
{
    fn from(x: Option<T>) -> Value {
        match x {
            Some(x) => x.into(),
            None => Value::Null,
        }
    }
}

lazy_static! {
    /// The name of the id attribute, `"id"`
    pub static ref ID: Word = Word::from("id");
}

/// An entity is represented as a map of attribute names to values.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct Entity(Object<Value>);

impl<'a> IntoIterator for &'a Entity {
    type Item = (Word, Value);

    type IntoIter = intern::ObjectOwningIter<Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.clone().into_iter()
    }
}

pub trait IntoEntityIterator: IntoIterator<Item = (Word, Value)> {}

impl<T: IntoIterator<Item = (Word, Value)>> IntoEntityIterator for T {}

pub trait TryIntoEntityIterator<E>: IntoIterator<Item = Result<(Word, Value), E>> {}

impl<E, T: IntoIterator<Item = Result<(Word, Value), E>>> TryIntoEntityIterator<E> for T {}

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum EntityValidationError {
    #[error("Entity {entity}[{id}]: unknown entity type `{entity}`")]
    UnknownEntityType { entity: String, id: String },

    #[error("Entity {entity}[{entity_id}]: field `{field}` is of type {expected_type}, but the value `{value}` contains a {actual_type} at index {index}")]
    MismatchedElementTypeInList {
        entity: String,
        entity_id: String,
        field: String,
        expected_type: String,
        value: String,
        actual_type: String,
        index: usize,
    },

    #[error("Entity {entity}[{entity_id}]: the value `{value}` for field `{field}` must have type {expected_type} but has type {actual_type}")]
    InvalidFieldType {
        entity: String,
        entity_id: String,
        value: String,
        field: String,
        expected_type: String,
        actual_type: String,
    },

    #[error("Entity {entity}[{entity_id}]: missing value for non-nullable field `{field}`")]
    MissingValueForNonNullableField {
        entity: String,
        entity_id: String,
        field: String,
    },

    #[error("Entity {entity}[{entity_id}]: field `{field}` is derived and cannot be set")]
    CannotSetDerivedField {
        entity: String,
        entity_id: String,
        field: String,
    },

    #[error("Unknown key `{0}`. It probably is not part of the schema")]
    UnknownKey(String),

    #[error("Internal error: no id attribute for entity `{entity}`")]
    MissingIDAttribute { entity: String },

    #[error("Unsupported type for `id` attribute")]
    UnsupportedTypeForIDAttribute,
}

/// The `entity!` macro is a convenient way to create entities in tests. It
/// can not be used in production code since it panics when creating the
/// entity goes wrong.
///
/// The macro takes a schema and a list of attribute names and values:
/// ```
///   use graph::entity;
///   use graph::schema::InputSchema;
///   use graph::data::subgraph::DeploymentHash;
///
///   let id = DeploymentHash::new("Qm123").unwrap();
///   let schema = InputSchema::parse("type User @entity { id: String!, name: String! }", id).unwrap();
///
///   let entity = entity! { schema => id: "1", name: "John Doe" };
/// ```
#[cfg(debug_assertions)]
#[macro_export]
macro_rules! entity {
    ($schema:expr => $($name:ident: $value:expr,)*) => {
        {
            let mut result = Vec::new();
            $(
                result.push(($crate::data::value::Word::from(stringify!($name)), $crate::data::store::Value::from($value)));
            )*
            $schema.make_entity(result).unwrap()
        }
    };
    ($schema:expr => $($name:ident: $value:expr),*) => {
        entity! {$schema => $($name: $value,)*}
    };
}

impl Entity {
    pub fn make<I: IntoEntityIterator>(
        pool: Arc<AtomPool>,
        iter: I,
    ) -> Result<Entity, EntityValidationError> {
        let mut obj = Object::new(pool);
        for (key, value) in iter {
            obj.insert(key, value)
                .map_err(|e| EntityValidationError::UnknownKey(e.not_interned()))?;
        }
        let entity = Entity(obj);
        entity.check_id()?;
        Ok(entity)
    }

    pub fn try_make<E: std::error::Error + Send + Sync + 'static, I: TryIntoEntityIterator<E>>(
        pool: Arc<AtomPool>,
        iter: I,
    ) -> Result<Entity, Error> {
        let mut obj = Object::new(pool);
        for pair in iter {
            let (key, value) = pair?;
            obj.insert(key, value)
                .map_err(|e| anyhow!("unknown attribute {}", e.not_interned()))?;
        }
        let entity = Entity(obj);
        entity.check_id()?;
        Ok(entity)
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    // This collects the entity into an ordered vector so that it can be iterated deterministically.
    pub fn sorted(self) -> Vec<(Word, Value)> {
        let mut v: Vec<_> = self.0.into_iter().map(|(k, v)| (k, v)).collect();
        v.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
        v
    }

    pub fn sorted_ref(&self) -> Vec<(&str, &Value)> {
        let mut v: Vec<_> = self.0.iter().collect();
        v.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
        v
    }

    fn check_id(&self) -> Result<(), EntityValidationError> {
        match self.get("id") {
            None => Err(EntityValidationError::MissingIDAttribute {
                entity: format!("{:?}", self.0),
            }),
            Some(Value::String(_)) | Some(Value::Bytes(_)) | Some(Value::Int8(_)) => Ok(()),
            _ => Err(EntityValidationError::UnsupportedTypeForIDAttribute),
        }
    }

    /// Return the ID of this entity. If the ID is a string, return the
    /// string. If it is `Bytes`, return it as a hex string with a `0x`
    /// prefix. If the ID is not set or anything but a `String` or `Bytes`,
    /// return an error
    pub fn id(&self) -> Id {
        Id::try_from(self.get("id").unwrap().clone()).expect("the id is set to a valid value")
    }

    /// Merges an entity update `update` into this entity.
    ///
    /// If a key exists in both entities, the value from `update` is chosen.
    /// If a key only exists on one entity, the value from that entity is chosen.
    /// If a key is set to `Value::Null` in `update`, the key/value pair is set to `Value::Null`.
    pub fn merge(&mut self, update: Entity) {
        self.0.merge(update.0);
    }

    /// Merges an entity update `update` into this entity, removing `Value::Null` values.
    ///
    /// If a key exists in both entities, the value from `update` is chosen.
    /// If a key only exists on one entity, the value from that entity is chosen.
    /// If a key is set to `Value::Null` in `update`, the key/value pair is removed.
    pub fn merge_remove_null_fields(&mut self, update: Entity) -> Result<(), InternError> {
        for (key, value) in update.0.into_iter() {
            match value {
                Value::Null => self.0.remove(&key),
                _ => self.0.insert(&key, value)?,
            };
        }
        Ok(())
    }

    /// Remove all entries with value `Value::Null` from `self`
    pub fn remove_null_fields(&mut self) {
        self.0.retain(|_, value| !value.is_null())
    }

    /// Add the key/value pairs from `iter` to this entity. This is the same
    /// as an implementation of `std::iter::Extend` would be, except that
    /// this operation is fallible because one of the keys from the iterator
    /// might not be in the underlying pool
    pub fn merge_iter(
        &mut self,
        iter: impl IntoIterator<Item = (impl AsRef<str>, Value)>,
    ) -> Result<(), InternError> {
        for (key, value) in iter {
            self.0.insert(key, value)?;
        }
        Ok(())
    }

    /// Validate that this entity matches the object type definition in the
    /// schema. An entity that passes these checks can be stored
    /// successfully in the subgraph's database schema
    pub fn validate(&self, key: &EntityKey) -> Result<(), EntityValidationError> {
        if key.entity_type.is_poi() {
            // Users can't modify Poi entities, and therefore they do not
            // need to be validated. In addition, the schema has no object
            // type for them, and validation would therefore fail
            return Ok(());
        }

        let object_type = key.entity_type.object_type().ok_or_else(|| {
            EntityValidationError::UnknownEntityType {
                entity: key.entity_type.to_string(),
                id: key.entity_id.to_string(),
            }
        })?;

        for field in object_type.fields.iter() {
            match (self.get(&field.name), field.is_derived) {
                (Some(value), false) => {
                    let scalar_type = &field.value_type;
                    if field.field_type.is_list() {
                        // Check for inhomgeneous lists to produce a better
                        // error message for them; other problems, like
                        // assigning a scalar to a list will be caught below
                        if let Value::List(elts) = value {
                            for (index, elt) in elts.iter().enumerate() {
                                if !elt.is_assignable(&scalar_type, false) {
                                    return Err(
                                        EntityValidationError::MismatchedElementTypeInList {
                                            entity: key.entity_type.to_string(),
                                            entity_id: key.entity_id.to_string(),
                                            field: field.name.to_string(),
                                            expected_type: field.field_type.to_string(),
                                            value: value.to_string(),
                                            actual_type: elt.type_name().to_string(),
                                            index,
                                        },
                                    );
                                }
                            }
                        }
                    }
                    if !value.is_assignable(&scalar_type, field.field_type.is_list()) {
                        return Err(EntityValidationError::InvalidFieldType {
                            entity: key.entity_type.to_string(),
                            entity_id: key.entity_id.to_string(),
                            value: value.to_string(),
                            field: field.name.to_string(),
                            expected_type: field.field_type.to_string(),
                            actual_type: value.type_name().to_string(),
                        });
                    }
                }
                (None, false) => {
                    if field.field_type.is_non_null() {
                        return Err(EntityValidationError::MissingValueForNonNullableField {
                            entity: key.entity_type.to_string(),
                            entity_id: key.entity_id.to_string(),
                            field: field.name.to_string(),
                        });
                    }
                }
                (Some(_), true) => {
                    return Err(EntityValidationError::CannotSetDerivedField {
                        entity: key.entity_type.to_string(),
                        entity_id: key.entity_id.to_string(),
                        field: field.name.to_string(),
                    });
                }
                (None, true) => {
                    // derived fields should not be set
                }
            }
        }
        Ok(())
    }
}

/// Convenience methods to modify individual attributes for tests.
/// Production code should not use/need this.
#[cfg(debug_assertions)]
impl Entity {
    pub fn insert(&mut self, key: &str, value: Value) -> Result<Option<Value>, InternError> {
        self.0.insert(key, value)
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.0.remove(key)
    }

    pub fn set(
        &mut self,
        name: &str,
        value: impl Into<Value>,
    ) -> Result<Option<Value>, InternError> {
        self.0.insert(name, value.into())
    }
}

impl<'a> From<&'a Entity> for Cow<'a, Entity> {
    fn from(entity: &'a Entity) -> Self {
        Cow::Borrowed(entity)
    }
}

impl CacheWeight for Entity {
    fn indirect_weight(&self) -> usize {
        self.0.indirect_weight()
    }
}

impl GasSizeOf for Entity {
    fn gas_size_of(&self) -> Gas {
        self.0.gas_size_of()
    }
}

impl std::fmt::Debug for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ds = f.debug_struct("Entity");
        for (k, v) in &self.0 {
            ds.field(k, v);
        }
        ds.finish()
    }
}

/// An object that is returned from a query. It's a an `r::Value` which
/// carries the attributes of the object (`__typename`, `id` etc.) and
/// possibly a pointer to its parent if the query that constructed it is one
/// that depends on parents
pub struct QueryObject {
    pub parent: Option<Id>,
    pub entity: r::Object,
}

impl CacheWeight for QueryObject {
    fn indirect_weight(&self) -> usize {
        self.parent.indirect_weight() + self.entity.indirect_weight()
    }
}

#[test]
fn value_bytes() {
    let graphql_value = r::Value::String("0x8f494c66afc1d3f8ac1b45df21f02a46".to_owned());
    let ty = q::Type::NamedType(BYTES_SCALAR.to_owned());
    let from_query = Value::from_query_value(&graphql_value, &ty).unwrap();
    assert_eq!(
        from_query,
        Value::Bytes(scalar::Bytes::from(
            &[143, 73, 76, 102, 175, 193, 211, 248, 172, 27, 69, 223, 33, 240, 42, 70][..]
        ))
    );
    assert_eq!(r::Value::from(from_query), graphql_value);
}

#[test]
fn value_bigint() {
    let big_num = "340282366920938463463374607431768211456";
    let graphql_value = r::Value::String(big_num.to_owned());
    let ty = q::Type::NamedType(BIG_INT_SCALAR.to_owned());
    let from_query = Value::from_query_value(&graphql_value, &ty).unwrap();
    assert_eq!(
        from_query,
        Value::BigInt(FromStr::from_str(big_num).unwrap())
    );
    assert_eq!(r::Value::from(from_query), graphql_value);
}

#[test]
fn entity_validation() {
    use crate::schema::InputSchema;

    const DOCUMENT: &str = "
    enum Color { red, yellow, blue }
    interface Stuff { id: ID!, name: String! }
    type Cruft @entity {
        id: ID!,
        thing: Thing!
    }
    type Thing @entity {
        id: ID!,
        name: String!,
        favorite_color: Color,
        stuff: Stuff,
        things: [Thing!]!
        # Make sure we do not validate derived fields; it's ok
        # to store a thing with a null Cruft
        cruft: Cruft! @derivedFrom(field: \"thing\")
    }";

    lazy_static! {
        static ref SUBGRAPH: DeploymentHash = DeploymentHash::new("doesntmatter").unwrap();
        static ref SCHEMA: InputSchema =
            InputSchema::parse(DOCUMENT, SUBGRAPH.clone()).expect("Failed to parse test schema");
        static ref THING_TYPE: EntityType = SCHEMA.entity_type("Thing").unwrap();
    }

    fn make_thing(name: &str) -> Entity {
        entity! { SCHEMA => id: name, name: name, stuff: "less", favorite_color: "red", things: Value::List(vec![]) }
    }

    fn check(thing: Entity, errmsg: &str) {
        let id = thing.id();
        let key = THING_TYPE.key(id.clone());

        let err = thing.validate(&key);
        if errmsg.is_empty() {
            assert!(
                err.is_ok(),
                "checking entity {}: expected ok but got {}",
                id,
                err.unwrap_err()
            );
        } else if let Err(e) = err {
            assert_eq!(errmsg, e.to_string(), "checking entity {}", id);
        } else {
            panic!(
                "Expected error `{}` but got ok when checking entity {}",
                errmsg, id
            );
        }
    }

    let mut thing = make_thing("t1");
    thing
        .set("things", Value::from(vec!["thing1", "thing2"]))
        .unwrap();
    check(thing, "");

    let thing = make_thing("t2");
    check(thing, "");

    let mut thing = make_thing("t3");
    thing.remove("name");
    check(
        thing,
        "Entity Thing[t3]: missing value for non-nullable field `name`",
    );

    let mut thing = make_thing("t4");
    thing.remove("things");
    check(
        thing,
        "Entity Thing[t4]: missing value for non-nullable field `things`",
    );

    let mut thing = make_thing("t5");
    thing.set("name", Value::Int(32)).unwrap();
    check(
        thing,
        "Entity Thing[t5]: the value `32` for field `name` must \
         have type String! but has type Int",
    );

    let mut thing = make_thing("t6");
    thing
        .set("things", Value::List(vec!["thing1".into(), 17.into()]))
        .unwrap();
    check(
        thing,
        "Entity Thing[t6]: field `things` is of type [Thing!]!, \
         but the value `[thing1, 17]` contains a Int at index 1",
    );

    let mut thing = make_thing("t7");
    thing.remove("favorite_color");
    thing.remove("stuff");
    check(thing, "");

    let mut thing = make_thing("t8");
    thing.set("cruft", "wat").unwrap();
    check(
        thing,
        "Entity Thing[t8]: field `cruft` is derived and cannot be set",
    );
}

#[test]
fn fmt_debug() {
    assert_eq!("String(\"hello\")", format!("{:?}", Value::from("hello")));
    assert_eq!("Int(17)", format!("{:?}", Value::Int(17)));
    assert_eq!("Bool(false)", format!("{:?}", Value::Bool(false)));
    assert_eq!("Null", format!("{:?}", Value::Null));

    let bd = Value::BigDecimal(scalar::BigDecimal::from(-0.17));
    assert_eq!("BigDecimal(-0.17)", format!("{:?}", bd));

    let bytes = Value::Bytes(scalar::Bytes::from([222, 173, 190, 239].as_slice()));
    assert_eq!("Bytes(0xdeadbeef)", format!("{:?}", bytes));

    let bi = Value::BigInt(scalar::BigInt::from(-17i32));
    assert_eq!("BigInt(-17)", format!("{:?}", bi));
}
