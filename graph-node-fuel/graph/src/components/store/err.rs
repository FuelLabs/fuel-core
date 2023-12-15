use super::{BlockNumber, DeploymentSchemaVersion};
use crate::prelude::QueryExecutionError;
use crate::util::intern::Error as InternError;
use crate::{data::store::EntityValidationError, prelude::DeploymentHash};

use anyhow::{anyhow, Error};
use diesel::result::Error as DieselError;
use thiserror::Error;
use tokio::task::JoinError;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("store error: {0:#}")]
    Unknown(Error),
    #[error("Entity validation failed: {0}")]
    EntityValidationError(EntityValidationError),
    #[error(
        "tried to set entity of type `{0}` with ID \"{1}\" but an entity of type `{2}`, \
         which has an interface in common with `{0}`, exists with the same ID"
    )]
    ConflictingId(String, String, String), // (entity, id, conflicting_entity)
    #[error("unknown field '{0}'")]
    UnknownField(String),
    #[error("unknown table '{0}'")]
    UnknownTable(String),
    #[error("entity type '{0}' does not have an attribute '{0}'")]
    UnknownAttribute(String, String),
    #[error("malformed directive '{0}'")]
    MalformedDirective(String),
    #[error("query execution failed: {0}")]
    QueryExecutionError(String),
    #[error("Child filter nesting not supported by value `{0}`: `{1}`")]
    ChildFilterNestingNotSupportedError(String, String),
    #[error("invalid identifier: {0}")]
    InvalidIdentifier(String),
    #[error(
        "subgraph `{0}` has already processed block `{1}`; \
         there are most likely two (or more) nodes indexing this subgraph"
    )]
    DuplicateBlockProcessing(DeploymentHash, BlockNumber),
    /// An internal error where we expected the application logic to enforce
    /// some constraint, e.g., that subgraph names are unique, but found that
    /// constraint to not hold
    #[error("internal constraint violated: {0}")]
    ConstraintViolation(String),
    #[error("deployment not found: {0}")]
    DeploymentNotFound(String),
    #[error("shard not found: {0} (this usually indicates a misconfiguration)")]
    UnknownShard(String),
    #[error("Fulltext search not yet deterministic")]
    FulltextSearchNonDeterministic,
    #[error("operation was canceled")]
    Canceled,
    #[error("database unavailable")]
    DatabaseUnavailable,
    #[error("database disabled")]
    DatabaseDisabled,
    #[error("subgraph forking failed: {0}")]
    ForkFailure(String),
    #[error("subgraph writer poisoned by previous error")]
    Poisoned,
    #[error("panic in subgraph writer: {0}")]
    WriterPanic(JoinError),
    #[error(
        "found schema version {0} but this graph node only supports versions up to {}. \
         Did you downgrade Graph Node?",
        DeploymentSchemaVersion::LATEST
    )]
    UnsupportedDeploymentSchemaVersion(i32),
    #[error("pruning failed: {0}")]
    PruneFailure(String),
}

// Convenience to report a constraint violation
#[macro_export]
macro_rules! constraint_violation {
    ($msg:expr) => {{
        $crate::prelude::StoreError::ConstraintViolation(format!("{}", $msg))
    }};
    ($fmt:expr, $($arg:tt)*) => {{
        $crate::prelude::StoreError::ConstraintViolation(format!($fmt, $($arg)*))
    }}
}

/// We can't derive `Clone` because some variants use non-cloneable data.
/// For those cases, produce an `Unknown` error with some details about the
/// original error
impl Clone for StoreError {
    fn clone(&self) -> Self {
        match self {
            Self::Unknown(arg0) => Self::Unknown(anyhow!("{}", arg0)),
            Self::EntityValidationError(arg0) => Self::EntityValidationError(arg0.clone()),
            Self::ConflictingId(arg0, arg1, arg2) => {
                Self::ConflictingId(arg0.clone(), arg1.clone(), arg2.clone())
            }
            Self::UnknownField(arg0) => Self::UnknownField(arg0.clone()),
            Self::UnknownTable(arg0) => Self::UnknownTable(arg0.clone()),
            Self::UnknownAttribute(arg0, arg1) => {
                Self::UnknownAttribute(arg0.clone(), arg1.clone())
            }
            Self::MalformedDirective(arg0) => Self::MalformedDirective(arg0.clone()),
            Self::QueryExecutionError(arg0) => Self::QueryExecutionError(arg0.clone()),
            Self::ChildFilterNestingNotSupportedError(arg0, arg1) => {
                Self::ChildFilterNestingNotSupportedError(arg0.clone(), arg1.clone())
            }
            Self::InvalidIdentifier(arg0) => Self::InvalidIdentifier(arg0.clone()),
            Self::DuplicateBlockProcessing(arg0, arg1) => {
                Self::DuplicateBlockProcessing(arg0.clone(), arg1.clone())
            }
            Self::ConstraintViolation(arg0) => Self::ConstraintViolation(arg0.clone()),
            Self::DeploymentNotFound(arg0) => Self::DeploymentNotFound(arg0.clone()),
            Self::UnknownShard(arg0) => Self::UnknownShard(arg0.clone()),
            Self::FulltextSearchNonDeterministic => Self::FulltextSearchNonDeterministic,
            Self::Canceled => Self::Canceled,
            Self::DatabaseUnavailable => Self::DatabaseUnavailable,
            Self::DatabaseDisabled => Self::DatabaseDisabled,
            Self::ForkFailure(arg0) => Self::ForkFailure(arg0.clone()),
            Self::Poisoned => Self::Poisoned,
            Self::WriterPanic(arg0) => Self::Unknown(anyhow!("writer panic: {}", arg0)),
            Self::UnsupportedDeploymentSchemaVersion(arg0) => {
                Self::UnsupportedDeploymentSchemaVersion(arg0.clone())
            }
            Self::PruneFailure(arg0) => Self::PruneFailure(arg0.clone()),
        }
    }
}

impl From<DieselError> for StoreError {
    fn from(e: DieselError) -> Self {
        // When the error is caused by a closed connection, treat the error
        // as 'database unavailable'. When this happens during indexing, the
        // indexing machinery will retry in that case rather than fail the
        // subgraph
        if let DieselError::DatabaseError(_, info) = &e {
            if info
                .message()
                .contains("server closed the connection unexpectedly")
            {
                return StoreError::DatabaseUnavailable;
            }
        }
        StoreError::Unknown(e.into())
    }
}

impl From<::diesel::r2d2::PoolError> for StoreError {
    fn from(e: ::diesel::r2d2::PoolError) -> Self {
        StoreError::Unknown(e.into())
    }
}

impl From<Error> for StoreError {
    fn from(e: Error) -> Self {
        StoreError::Unknown(e)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError::Unknown(e.into())
    }
}

impl From<QueryExecutionError> for StoreError {
    fn from(e: QueryExecutionError) -> Self {
        StoreError::QueryExecutionError(e.to_string())
    }
}

impl From<std::fmt::Error> for StoreError {
    fn from(e: std::fmt::Error) -> Self {
        StoreError::Unknown(anyhow!("{}", e.to_string()))
    }
}

impl From<InternError> for StoreError {
    fn from(e: InternError) -> Self {
        match e {
            InternError::NotInterned(key) => StoreError::UnknownField(key),
        }
    }
}
