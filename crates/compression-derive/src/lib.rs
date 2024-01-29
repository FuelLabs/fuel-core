//! Derive macros for canonical type serialization and deserialization.

#![deny(unused_must_use, missing_docs)]

extern crate proc_macro;
mod attribute;
mod deserialize;
mod serialize;

use self::{
    deserialize::deserialize_derive,
    serialize::serialize_derive,
};

synstructure::decl_derive!(
    [Deserialize, attributes(da_compress)] =>
    /// Derives `Deserialize` trait for the given `struct` or `enum`.
    deserialize_derive
);
synstructure::decl_derive!(
    [Serialize, attributes(da_compress)] =>
    /// Derives `Serialize` trait for the given `struct` or `enum`.
    serialize_derive
);
