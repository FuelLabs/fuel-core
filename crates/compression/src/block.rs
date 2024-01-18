use serde::{
    Deserialize,
    Serialize,
};

use crate::registry::Registrations;

/// Compressed block.
/// The versioning here working depends on the serialization format,
/// but as long as we we have less than 128 variants, postcard will
/// make that a single byte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CompressedBlock {
    V0 {
        /// Registration section of the compressed block
        registrations: Registrations,
    },
}
