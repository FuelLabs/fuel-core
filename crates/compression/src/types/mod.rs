use serde::{
    Deserialize,
    Serialize,
};

use crate::registry::Key;

pub(crate) mod header;
pub(crate) mod tx;

/// For types that need an explicit flag marking them
/// references to the registry instead of raw values,
/// this enum can be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaybeCompressed<T> {
    Compressed(Key),
    Uncompressed(T),
}
