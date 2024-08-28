//! The crate `fuel-core-storage` contains storage types, primitives, tables used by `fuel-core`.
//! This crate doesn't contain the actual implementation of the storage. It works around the
//! `Database` and is used by services to provide a default implementation. Primitives
//! defined here are used by services but are flexible enough to customize the
//! logic when the `Database` is known.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]
#![deny(warnings)]

#[cfg(feature = "alloc")]
extern crate alloc;

use anyhow::anyhow;
use core::array::TryFromSliceError;
use fuel_core_types::services::executor::Error as ExecutorError;

#[cfg(feature = "alloc")]
use alloc::{
    boxed::Box,
    string::ToString,
};

pub use fuel_vm_private::{
    fuel_storage::*,
    storage::{
        ContractsAssetsStorage,
        InterpreterStorage,
    },
};

pub mod blueprint;
pub mod codec;
pub mod column;
pub mod iter;
pub mod kv_store;
pub mod structured_storage;
pub mod tables;
#[cfg(feature = "test-helpers")]
pub mod test_helpers;
pub mod transactional;
pub mod vm_storage;

use fuel_core_types::fuel_merkle::binary::MerkleTreeError;
pub use fuel_vm_private::storage::{
    ContractsAssetKey,
    ContractsStateData,
    ContractsStateKey,
};
#[doc(hidden)]
pub use paste;
#[cfg(feature = "test-helpers")]
#[doc(hidden)]
pub use rand;

/// The storage result alias.
pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, derive_more::Display, derive_more::From)]
#[non_exhaustive]
/// Error occurring during interaction with storage
pub enum Error {
    /// Error occurred during serialization or deserialization of the entity.
    #[display(fmt = "error performing serialization or deserialization `{_0}`")]
    Codec(anyhow::Error),
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

#[cfg(feature = "test-helpers")]
impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        self.to_string().eq(&other.to_string())
    }
}

impl From<Error> for anyhow::Error {
    fn from(error: Error) -> Self {
        anyhow::Error::msg(error)
    }
}

impl From<TryFromSliceError> for Error {
    fn from(e: TryFromSliceError) -> Self {
        Self::Other(anyhow::anyhow!(e))
    }
}

impl From<Error> for ExecutorError {
    fn from(e: Error) -> Self {
        ExecutorError::StorageError(e.to_string())
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

impl From<MerkleTreeError<Error>> for Error {
    fn from(e: MerkleTreeError<Error>) -> Self {
        match e {
            MerkleTreeError::StorageError(s) => s,
            e => Error::Other(anyhow!(e)),
        }
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

/// The traits allow work with the storage in batches.
/// Some implementations can perform batch operations faster than one by one.
#[impl_tools::autoimpl(for<T: trait> &mut T)]
pub trait StorageBatchMutate<Type: Mappable>: StorageMutate<Type> {
    /// Initialize the storage with batch insertion. This method is more performant than
    /// [`Self::insert_batch`] in some cases.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage is already initialized.
    fn init_storage<'a, Iter>(&mut self, set: Iter) -> Result<()>
    where
        Iter: 'a + Iterator<Item = (&'a Type::Key, &'a Type::Value)>,
        Type::Key: 'a,
        Type::Value: 'a;

    /// Inserts the key-value pair into the storage in batch.
    fn insert_batch<'a, Iter>(&mut self, set: Iter) -> Result<()>
    where
        Iter: 'a + Iterator<Item = (&'a Type::Key, &'a Type::Value)>,
        Type::Key: 'a,
        Type::Value: 'a;

    /// Removes the key-value pairs from the storage in batch.
    fn remove_batch<'a, Iter>(&mut self, set: Iter) -> Result<()>
    where
        Iter: 'a + Iterator<Item = &'a Type::Key>,
        Type::Key: 'a;
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
