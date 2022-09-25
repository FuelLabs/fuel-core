use crate::{
    database::Column,
    state::in_memory::transaction::MemoryTransactionView,
};
use fuel_core_interfaces::common::fuel_tx::UtxoId;
use std::{
    fmt::Debug,
    marker::PhantomData,
    mem,
    sync::Arc,
};

pub use fuel_core_interfaces::db::Error;

pub type Result<T> = core::result::Result<T, Error>;
pub type DataSource = Arc<dyn TransactableStorage>;
pub type ColumnId = u32;

const UTXO_ID_SIZE: usize = 32 + 1;

pub trait UtxoIdAsRef {
    fn as_ref(&self) -> &[u8];
}

impl UtxoIdAsRef for UtxoId {
    fn as_ref(&self) -> &[u8] {
        let array = unsafe { mem::transmute::<&UtxoId, &[u8; UTXO_ID_SIZE]>(self) };
        array.as_ref()
    }
}

#[macro_export]
macro_rules! multikey {
    ($left:expr, $left_ty:ty, $right:expr, $right_ty:ty) => {{
        use $crate::state::UtxoIdAsRef as _;

        let left: &$left_ty = $left;
        let right: &$right_ty = $right;
        const LSIZE: usize = <$left_ty>::LEN;
        const RSIZE: usize = <$right_ty>::LEN;
        const SIZE: usize = LSIZE + RSIZE;
        let mut key: [u8; SIZE] = [0; SIZE];
        key[..LSIZE].copy_from_slice(left.as_ref());
        key[LSIZE..].copy_from_slice(right.as_ref());
        key
    }};
}

pub type KVItem = Result<(Vec<u8>, Vec<u8>)>;

pub trait KeyValueStore {
    fn get(&self, key: &[u8], column: Column) -> Result<Option<Vec<u8>>>;
    fn put(&self, key: &[u8], column: Column, value: Vec<u8>) -> Result<Option<Vec<u8>>>;
    fn delete(&self, key: &[u8], column: Column) -> Result<Option<Vec<u8>>>;
    fn exists(&self, key: &[u8], column: Column) -> Result<bool>;
    /// Returns an iterator over all items of the table with `column`.
    /// The `prefix` allows iterating only over data with the corresponding prefix.
    /// The `start` specifies the start element of the iteration.
    ///
    /// # Note: `prefix` and `start` should be `Vec` because the iterator owns it.
    fn iter_all(
        &self,
        column: Column,
        prefix: Option<Vec<u8>>,
        start: Option<Vec<u8>>,
        direction: IterDirection,
    ) -> Box<dyn Iterator<Item = KVItem> + '_>;
}

#[derive(Copy, Clone, Debug, PartialOrd, Eq, PartialEq)]
pub enum IterDirection {
    Forward,
    Reverse,
}

impl Default for IterDirection {
    fn default() -> Self {
        Self::Forward
    }
}

pub trait BatchOperations: KeyValueStore {
    fn batch_write(
        &self,
        entries: &mut dyn Iterator<Item = WriteOperation>,
    ) -> Result<()> {
        for entry in entries {
            match entry {
                // TODO: error handling
                WriteOperation::Insert(key, column, value) => {
                    let _ = self.put(&key, column, value);
                }
                WriteOperation::Remove(key, column) => {
                    let _ = self.delete(&key, column);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum WriteOperation {
    Insert(Vec<u8>, Column, Vec<u8>),
    Remove(Vec<u8>, Column),
}

pub trait Transaction {
    fn transaction<F, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut MemoryTransactionView) -> TransactionResult<R> + Copy;
}

pub type TransactionResult<T> = core::result::Result<T, TransactionError>;

pub trait TransactableStorage:
    KeyValueStore + BatchOperations + Debug + Send + Sync
{
}

#[derive(Clone, Debug)]
pub enum TransactionError {
    Aborted,
}

pub mod in_memory;
#[cfg(feature = "rocksdb")]
pub mod rocks_db;
