use crate::client::{
    schema,
    types::primitives::{BlobId, Bytes},
};

#[derive(Debug, Clone, PartialEq)]
pub struct Blob {
    pub id: BlobId,
    pub bytecode: Bytes,
}

// GraphQL Translation

impl From<schema::blob::Blob> for Blob {
    fn from(value: schema::blob::Blob) -> Self {
        Self {
            id: value.id.into(),
            bytecode: value.bytecode.into(),
        }
    }
}
