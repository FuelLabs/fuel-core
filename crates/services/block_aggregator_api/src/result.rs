use thiserror::Error;
#[derive(Debug, Error)]
pub enum Error {
    #[error("Block Aggregator API error")]
    Api,
    #[error("Block Source error: {0}")]
    BlockSource(anyhow::Error),
    #[error("Database error: {0}")]
    DB(anyhow::Error),
}

pub type Result<T, E = Error> = core::result::Result<T, E>;
