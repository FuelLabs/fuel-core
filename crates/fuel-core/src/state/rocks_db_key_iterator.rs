use crate::database::{
    Error as DatabaseError,
    Result as DatabaseResult,
};
use anyhow::anyhow;

use fuel_core_storage::iter::IterDirection;
use rocksdb::{
    DBAccess,
    DBRawIteratorWithThreadMode,
};

pub enum OwnedIteratorMode {
    Start,
    End,
    From(Vec<u8>, rocksdb::Direction),
}

pub struct RocksDBKeyIterator<'a, D: DBAccess> {
    raw: DBRawIteratorWithThreadMode<'a, D>,
    direction: IterDirection,
    done: bool,
}

impl<'a, D: DBAccess> Iterator for RocksDBKeyIterator<'a, D> {
    // decoupling the Key type from crate::storage::KeyItem
    type Item = DatabaseResult<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            None
        } else if let Some(key) = self.raw.key() {
            let key = key.to_vec();
            match self.direction {
                IterDirection::Forward => self.raw.next(),
                IterDirection::Reverse => self.raw.prev(),
            }
            Some(DatabaseResult::Ok(key))
        } else {
            self.done = true;
            self.raw
                .status()
                .err()
                .map(|e| Some(DatabaseResult::Err(DatabaseError::Other(anyhow!(e)))))?
        }
    }
}

impl<'a, D: DBAccess> RocksDBKeyIterator<'a, D> {
    fn set_mode(&mut self, mode: OwnedIteratorMode) {
        self.done = false;
        self.direction = match mode {
            OwnedIteratorMode::Start => {
                self.raw.seek_to_first();
                IterDirection::Forward
            }
            OwnedIteratorMode::End => {
                self.raw.seek_to_last();
                IterDirection::Reverse
            }
            OwnedIteratorMode::From(key, rocksdb::Direction::Forward) => {
                self.raw.seek(key);
                IterDirection::Forward
            }
            OwnedIteratorMode::From(key, rocksdb::Direction::Reverse) => {
                self.raw.seek_for_prev(key);
                IterDirection::Reverse
            }
        };
    }

    pub fn new(raw: DBRawIteratorWithThreadMode<'a, D>, mode: OwnedIteratorMode) -> Self {
        let mut raw_iterator = Self {
            raw,
            direction: IterDirection::Forward, // overwritten by set_mode
            done: false,
        };
        raw_iterator.set_mode(mode);
        raw_iterator
    }
}
