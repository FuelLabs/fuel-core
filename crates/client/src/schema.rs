#![allow(clippy::derive_partial_eq_without_eq)]
use serde::{Deserialize, Serialize};

include! {concat!(env!("OUT_DIR"), "/../schema.rs")}
