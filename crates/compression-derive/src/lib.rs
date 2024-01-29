//! Derive macros for canonical type serialization and deserialization.

#![deny(unused_must_use, missing_docs)]

extern crate proc_macro;
mod attribute;
mod compact;

use self::compact::compact_derive;

synstructure::decl_derive!(
    [Compact, attributes(da_compress)] =>
    /// Derives `Compact` trait for the given `struct` or `enum`.
    compact_derive
);
