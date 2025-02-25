use fuel_core_storage::{
    Error as StorageError,
    MerkleRoot,
};
use fuel_core_types::{
    blockchain::primitives::BlockId,
    fuel_types::BlockHeight,
    services::executor,
};
use tokio::sync::{
    mpsc,
    oneshot,
    TryAcquireError,
};

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display(fmt = "The commit is already in the progress: {_0}.")]
    Semaphore(TryAcquireError),
    #[display(
        fmt = "The wrong state of database during insertion of the genesis block."
    )]
    InvalidUnderlyingDatabaseGenesisState,
    #[display(fmt = "The wrong state of storage after execution of the block.\
        The actual root is {_1:?}, when the expected root is {_0:?}.")]
    InvalidDatabaseStateAfterExecution(Option<MerkleRoot>, Option<MerkleRoot>),
    #[display(fmt = "Got overflow during increasing the height.")]
    Overflow,
    #[display(fmt = "The non-generic block can't have zero height.")]
    ZeroNonGenericHeight,
    #[display(fmt = "The actual height is {_1}, when the next expected height is {_0}.")]
    IncorrectBlockHeight(BlockHeight, BlockHeight),
    #[display(
        fmt = "Got another block id after validation of the block. Expected {_0} != Actual {_1}"
    )]
    BlockIdMismatch(BlockId, BlockId),
    #[display(fmt = "Some of the block fields are not valid: {_0}.")]
    FailedVerification(anyhow::Error),
    #[display(fmt = "The execution of the block failed: {_0}.")]
    FailedExecution(executor::Error),
    #[display(fmt = "It is not possible to execute the genesis block.")]
    ExecuteGenesis,
    #[display(fmt = "The database already contains the data at the height {_0}.")]
    NotUnique(BlockHeight),
    #[display(fmt = "The previous block processing is not finished yet.")]
    PreviousBlockProcessingNotFinished,
    #[display(fmt = "The send command to the inner task failed.")]
    SendCommandToInnerTaskFailed,
    #[display(fmt = "The inner import task is not running.")]
    InnerTaskIsNotRunning,
    #[from]
    Storage(StorageError),
    UnsupportedConsensusVariant(String),
    ActiveBlockResultsSemaphoreClosed(tokio::sync::AcquireError),
    RayonTaskWasCanceled,
}

impl From<Error> for anyhow::Error {
    fn from(error: Error) -> Self {
        anyhow::Error::msg(error)
    }
}

impl From<oneshot::error::RecvError> for Error {
    fn from(_: oneshot::error::RecvError) -> Self {
        Error::InnerTaskIsNotRunning
    }
}

impl<T> From<mpsc::error::SendError<T>> for Error {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        Error::SendCommandToInnerTaskFailed
    }
}

#[cfg(test)]
impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        format!("{self}") == format!("{other}")
    }
}
