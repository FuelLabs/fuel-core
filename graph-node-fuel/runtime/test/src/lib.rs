#![cfg(test)]
pub mod common;
mod test;

#[cfg(test)]
pub mod test_padding;

// this used in crate::test_padding module
// graph_runtime_derive::generate_from_rust_type looks for types in crate::protobuf,
// hence this mod presence in crate that uses ASC related macros is required
pub mod protobuf {
    pub use super::test_padding::data::*;
}
