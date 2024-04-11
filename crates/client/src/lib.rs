#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]
pub mod client;
pub mod schema;

/// The GraphQL schema used by the library.
pub const SCHEMA_SDL: &[u8] = include_bytes!("../assets/schema.sdl");
