use thiserror::Error;
#[derive(Debug, Error)]
pub enum Error {
    #[error("Block Aggregator API error: {0}")]
    Api(anyhow::Error),
    #[error("Block Source error: {0}")]
    BlockSource(anyhow::Error),
    #[error("Database error: {0}")]
    DB(anyhow::Error),
    #[error("Serialization error: {0}")]
    Serialization(anyhow::Error),
}

impl Error {
    pub fn db_error<T: Into<anyhow::Error>>(err: T) -> Self {
        Error::DB(err.into())
    }

    pub fn block_source_error<T: Into<anyhow::Error>>(err: T) -> Self {
        Error::BlockSource(err.into())
    }
}

pub type Result<T, E = Error> = core::result::Result<T, E>;
