use graph::data::subgraph::schema::SubgraphError;
use graph::prelude::{thiserror, Error, StoreError};

#[derive(thiserror::Error, Debug)]
pub enum BlockProcessingError {
    #[error("{0:#}")]
    Unknown(#[from] Error),

    // The error had a deterministic cause but, for a possibly non-deterministic reason, we chose to
    // halt processing due to the error.
    #[error("{0}")]
    Deterministic(SubgraphError),

    #[error("subgraph stopped while processing triggers")]
    Canceled,
}

impl BlockProcessingError {
    pub fn is_deterministic(&self) -> bool {
        matches!(self, BlockProcessingError::Deterministic(_))
    }
}

impl From<StoreError> for BlockProcessingError {
    fn from(e: StoreError) -> Self {
        BlockProcessingError::Unknown(e.into())
    }
}
