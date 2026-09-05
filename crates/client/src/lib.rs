#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]
#[cfg(all(test, not(feature = "subscriptions")))]
use {
    mockito as _,
    tokio as _,
};
pub mod client;
pub mod reqwest_ext;
pub mod schema;
pub mod transport;

/// The GraphQL schema used by the library.
pub const SCHEMA_SDL: &[u8] = include_bytes!("../assets/schema.sdl");
