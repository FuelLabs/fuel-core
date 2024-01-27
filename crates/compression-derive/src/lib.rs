//! Derive macros for canonical type serialization and deserialization.

#![deny(unused_must_use, missing_docs)]

extern crate proc_macro;
mod deserialize;
mod serialize;

use self::{
    deserialize::deserialize_derive,
    serialize::serialize_derive,
};
synstructure::decl_derive!(
    [Deserialize, attributes(canonical)] =>
    /// Derives `Deserialize` trait for the given `struct` or `enum`.
    deserialize_derive
);
synstructure::decl_derive!(
    [Serialize, attributes(canonical)] =>
    /// Derives `Serialize` trait for the given `struct` or `enum`.
    serialize_derive
);
