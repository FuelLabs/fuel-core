mod serialization;

/// Traits to navigate the GraphQL AST
pub mod ext;
pub use ext::{DirectiveExt, DocumentExt, ObjectTypeExt, TypeExt, ValueExt};

/// Utilities for working with GraphQL values.
mod values;

/// Serializable wrapper around a GraphQL value.
pub use self::serialization::SerializableValue;

pub use self::values::{
    // Trait for converting from GraphQL values into other types.
    TryFromValue,

    // Trait for plucking typed values from a GraphQL list.
    ValueList,

    // Trait for plucking typed values out of a GraphQL value maps.
    ValueMap,
};

pub mod shape_hash;

pub mod load_manager;

pub mod object_or_interface;
pub use object_or_interface::ObjectOrInterface;

pub mod object_macro;
pub use crate::object;
pub use object_macro::{object_value, IntoValue};
