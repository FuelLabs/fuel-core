//! The crate `fuel-core-storage` contains storage types, primitives, tables used by `fuel-core`.
//! This crate doesn't contain the actual implementation of the storage. It works around the
//! `Database` and is used by services to provide a default implementation. Primitives
//! defined here are used by services but are flexible enough to customize the
//! logic when the `Database` is known.

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]
#![deny(warnings)]

use fuel_core_types::services::executor::Error as ExecutorError;

pub use fuel_vm_private::{
    fuel_storage::*,
    storage::{
        ContractsAssetsStorage,
        InterpreterStorage,
    },
};

pub mod iter;
pub mod tables;
#[cfg(feature = "test-helpers")]
pub mod test_helpers;
pub mod transactional;

pub use fuel_vm_private::storage::{
    ContractsAssetKey,
    ContractsStateKey,
};

/// The storage result alias.
pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, derive_more::Display, derive_more::From)]
#[non_exhaustive]
/// Error occurring during interaction with storage
pub enum Error {
    /// Error occurred during serialization or deserialization of the entity.
    #[display(fmt = "error performing serialization or deserialization")]
    Codec,
    /// Error occurred during interaction with database.
    #[display(fmt = "error occurred in the underlying datastore `{_0:?}`")]
    DatabaseError(Box<dyn core::fmt::Debug + Send + Sync>),
    /// This error should be created with `not_found` macro.
    #[display(fmt = "resource of type `{_0}` was not found at the: {_1}")]
    NotFound(&'static str, &'static str),
    // TODO: Do we need this type at all?
    /// Unknown or not expected(by architecture) error.
    #[from]
    Other(anyhow::Error),
}

impl From<Error> for anyhow::Error {
    fn from(error: Error) -> Self {
        anyhow::Error::msg(error)
    }
}

impl From<Error> for ExecutorError {
    fn from(e: Error) -> Self {
        ExecutorError::StorageError(anyhow::anyhow!(e))
    }
}

impl From<Error> for fuel_vm_private::prelude::InterpreterError<Error> {
    fn from(e: Error) -> Self {
        fuel_vm_private::prelude::InterpreterError::Storage(e)
    }
}

impl From<Error> for fuel_vm_private::prelude::RuntimeError<Error> {
    fn from(e: Error) -> Self {
        fuel_vm_private::prelude::RuntimeError::Storage(e)
    }
}

/// The helper trait to work with storage errors.
pub trait IsNotFound {
    /// Return `true` if the error is [`Error::NotFound`].
    fn is_not_found(&self) -> bool;
}

impl IsNotFound for Error {
    fn is_not_found(&self) -> bool {
        matches!(self, Error::NotFound(_, _))
    }
}

impl<T> IsNotFound for Result<T> {
    fn is_not_found(&self) -> bool {
        match self {
            Err(err) => err.is_not_found(),
            _ => false,
        }
    }
}

/// Creates `StorageError::NotFound` error with file and line information inside.
///
/// # Examples
///
/// ```
/// use fuel_core_storage::not_found;
/// use fuel_core_storage::tables::Messages;
///
/// let string_type = not_found!("BlockId");
/// let mappable_type = not_found!(Messages);
/// let mappable_path = not_found!(fuel_core_storage::tables::Messages);
/// ```
#[macro_export]
macro_rules! not_found {
    ($name: literal) => {
        $crate::Error::NotFound($name, concat!(file!(), ":", line!()))
    };
    ($ty: path) => {
        $crate::Error::NotFound(
            ::core::any::type_name::<<$ty as $crate::Mappable>::OwnedValue>(),
            concat!(file!(), ":", line!()),
        )
    };
}

#[cfg(test)]
mod test {
    use crate::tables::Coins;

    #[test]
    fn not_found_output() {
        #[rustfmt::skip]
        assert_eq!(
            format!("{}", not_found!("BlockId")),
            format!("resource of type `BlockId` was not found at the: {}:{}", file!(), line!() - 1)
        );
        #[rustfmt::skip]
        assert_eq!(
            format!("{}", not_found!(Coins)),
            format!("resource of type `fuel_core_types::entities::coins::coin::CompressedCoin` was not found at the: {}:{}", file!(), line!() - 1)
        );
    }
}
