use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    registry::Registrations,
    types,
};

/// Compressed block.
/// The versioning here working depends on the serialization format,
/// but as long as we we have less than 128 variants, postcard will
/// make that a single byte.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CompressedBlock {
    V0 {
        /// Registration section of the compressed block
        registrations: Registrations,
        /// Compressed block header
        header: types::header::Header,
        /// Compressed transactions
        transactions: Vec<types::tx::Transaction>,
    },
}
