use crate::database::{
    Error as DatabaseError,
    Result as DatabaseResult,
};
use anyhow::anyhow;

use fuel_core_storage::iter::IterDirection;
use rocksdb::{
    DBAccess,
    DBRawIteratorWithThreadMode,
    IteratorMode,
};

pub struct RocksDBKeyIterator<'a, D: DBAccess, R> {
    raw: DBRawIteratorWithThreadMode<'a, D>,
    direction: IterDirection,
    done: bool,
    _marker: core::marker::PhantomData<R>,
}

pub trait ExtractItem: 'static {
    /// The item type returned by the iterator.
    type Item;

    /// Extracts the item from the raw iterator.
    fn extract_item<D>(
        raw_iterator: &DBRawIteratorWithThreadMode<D>,
    ) -> Option<Self::Item>
    where
        D: DBAccess;

    /// Returns the size of the item.
    fn size(item: &Self::Item) -> u64;

    /// Checks if the item starts with the given prefix.
    fn starts_with(item: &Self::Item, prefix: &[u8]) -> bool;
}

impl<'a, D: DBAccess, R> Iterator for RocksDBKeyIterator<'a, D, R>
where
    R: ExtractItem,
{
    // decoupling the Key type from crate::storage::KeyItem
    type Item = DatabaseResult<R::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            None
        } else if let Some(item) = R::extract_item(&self.raw) {
            match self.direction {
                IterDirection::Forward => self.raw.next(),
                IterDirection::Reverse => self.raw.prev(),
            }
            Some(DatabaseResult::Ok(item))
        } else {
            self.done = true;
            self.raw
                .status()
                .err()
                .map(|e| Some(DatabaseResult::Err(DatabaseError::Other(anyhow!(e)))))?
        }
    }
}

impl<'a, D: DBAccess, R> RocksDBKeyIterator<'a, D, R> {
    fn set_mode(&mut self, mode: IteratorMode) {
        self.done = false;
        self.direction = match mode {
            IteratorMode::Start => {
                self.raw.seek_to_first();
                IterDirection::Forward
            }
            IteratorMode::End => {
                self.raw.seek_to_last();
                IterDirection::Reverse
            }
            IteratorMode::From(key, rocksdb::Direction::Forward) => {
                self.raw.seek(key);
                IterDirection::Forward
            }
            IteratorMode::From(key, rocksdb::Direction::Reverse) => {
                self.raw.seek_for_prev(key);
                IterDirection::Reverse
            }
        };
    }

    pub fn new(
        raw: DBRawIteratorWithThreadMode<'a, D>,
        mode: IteratorMode,
    ) -> RocksDBKeyIterator<'a, D, R> {
        let mut raw_iterator = Self {
            raw,
            direction: IterDirection::Forward, // overwritten by set_mode
            done: false,
            _marker: Default::default(),
        };
        raw_iterator.set_mode(mode);
        raw_iterator
    }
}
