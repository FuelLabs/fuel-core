use crate::state::KVItem;
use crate::{
    database::{
        columns,
        columns::METADATA,
        metadata::{DB_VERSION, DB_VERSION_KEY},
    },
    state::{
        BatchOperations, ColumnId, Error, IterDirection, KeyValueStore, TransactableStorage,
        WriteOperation,
    },
};
#[cfg(feature = "metrics")]
use fuel_metrics::core_metrics::DATABASE_METRICS;
use rocksdb::{
    BoundColumnFamily, ColumnFamilyDescriptor, DBCompressionType, DBWithThreadMode, IteratorMode,
    MultiThreaded, Options, ReadOptions, SliceTransform, WriteBatch,
};
use std::{convert::TryFrom, path::Path, sync::Arc};

type DB = DBWithThreadMode<MultiThreaded>;
#[derive(Debug)]
pub struct RocksDb {
    db: DBWithThreadMode<MultiThreaded>,
}

impl RocksDb {
    pub fn open<P: AsRef<Path>>(path: P, cols: u32) -> Result<RocksDb, Error> {
        let cf_descriptors: Vec<_> = (0..cols)
            .map(|i| ColumnFamilyDescriptor::new(RocksDb::col_name(i), Self::cf_opts(i)))
            .collect();

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(DBCompressionType::Lz4);
        let db = match DB::open_cf_descriptors(&opts, &path, cf_descriptors) {
            Err(_) => {
                // setup cfs
                match DB::open_cf(&opts, &path, &[] as &[&str]) {
                    Ok(db) => {
                        for i in 0..cols {
                            db.create_cf(RocksDb::col_name(i), &opts)
                                .map_err(|e| Error::DatabaseError(Box::new(e)))?;
                        }
                        Ok(db)
                    }
                    err => err,
                }
            }
            ok => ok,
        }
        .map_err(|e| Error::DatabaseError(Box::new(e)))?;
        let rocks_db = RocksDb { db };
        rocks_db.validate_or_set_db_version()?;
        Ok(rocks_db)
    }

    fn validate_or_set_db_version(&self) -> Result<(), Error> {
        let data = self.get(DB_VERSION_KEY, METADATA)?;
        match data {
            None => {
                self.put(
                    DB_VERSION_KEY.to_vec(),
                    METADATA,
                    DB_VERSION.to_be_bytes().to_vec(),
                )?;
            }
            Some(v) => {
                let b =
                    <[u8; 4]>::try_from(v.as_slice()).map_err(|_| Error::InvalidDatabaseVersion)?;
                let version = u32::from_be_bytes(b);
                if version != DB_VERSION {
                    return Err(Error::InvalidDatabaseVersion);
                }
            }
        };
        Ok(())
    }

    fn cf(&self, column: ColumnId) -> Arc<BoundColumnFamily> {
        self.db
            .cf_handle(&*RocksDb::col_name(column))
            .expect("invalid column state")
    }

    fn col_name(column: ColumnId) -> String {
        format!("column-{}", column)
    }

    fn cf_opts(column: ColumnId) -> Options {
        let mut opts = Options::default();
        opts.create_if_missing(true);

        if column == columns::OWNED_COINS {
            // prefix is address length
            opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(32))
        }
        if column == columns::TRANSACTIONS_BY_OWNER_BLOCK_IDX {
            // prefix is address length
            opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(32))
        }

        opts
    }
}

impl KeyValueStore for RocksDb {
    fn get(&self, key: &[u8], column: ColumnId) -> crate::state::Result<Option<Vec<u8>>> {
        #[cfg(feature = "metrics")]
        DATABASE_METRICS.read_meter.inc();
        let value = self
            .db
            .get_cf(&self.cf(column), key)
            .map_err(|e| Error::DatabaseError(Box::new(e)));
        #[cfg(feature = "metrics")]
        {
            if value.is_ok() && value.as_ref().unwrap().is_some() {
                let value_as_vec = value.as_ref().cloned().unwrap().unwrap();
                DATABASE_METRICS
                    .bytes_read_meter
                    .inc_by(value_as_vec.len() as u64);
            }
        }
        value
    }

    fn put(
        &self,
        key: Vec<u8>,
        column: ColumnId,
        value: Vec<u8>,
    ) -> crate::state::Result<Option<Vec<u8>>> {
        #[cfg(feature = "metrics")]
        {
            DATABASE_METRICS.write_meter.inc();
            DATABASE_METRICS
                .bytes_written_meter
                .inc_by(value.len() as u64);
        }
        let prev = self.get(&key, column)?;
        self.db
            .put_cf(&self.cf(column), key, value)
            .map_err(|e| Error::DatabaseError(Box::new(e)))
            .map(|_| prev)
    }

    fn delete(&self, key: &[u8], column: ColumnId) -> crate::state::Result<Option<Vec<u8>>> {
        let prev = self.get(key, column)?;
        self.db
            .delete_cf(&self.cf(column), key)
            .map_err(|e| Error::DatabaseError(Box::new(e)))
            .map(|_| prev)
    }

    fn exists(&self, key: &[u8], column: ColumnId) -> crate::state::Result<bool> {
        // use pinnable mem ref to avoid memcpy of values associated with the key
        // since we're just checking for the existence of the key
        self.db
            .get_pinned_cf(&self.cf(column), key)
            .map_err(|e| Error::DatabaseError(Box::new(e)))
            .map(|v| v.is_some())
    }

    fn iter_all(
        &self,
        column: ColumnId,
        prefix: Option<Vec<u8>>,
        start: Option<Vec<u8>>,
        direction: IterDirection,
    ) -> Box<dyn Iterator<Item = KVItem> + '_> {
        let iter_mode = start.as_ref().map_or_else(
            || {
                prefix.as_ref().map_or_else(
                    || {
                        // if no start or prefix just start iterating over entire keyspace
                        match direction {
                            IterDirection::Forward => IteratorMode::Start,
                            // end always iterates in reverse
                            IterDirection::Reverse => IteratorMode::End,
                        }
                    },
                    // start iterating in a certain direction within the keyspace
                    |prefix| IteratorMode::From(prefix, direction.into()),
                )
            },
            // start iterating in a certain direction from the start key
            |k| IteratorMode::From(k, direction.into()),
        );

        let mut opts = ReadOptions::default();
        if prefix.is_some() {
            opts.set_prefix_same_as_start(true);
        }

        let iter = self
            .db
            .iterator_cf_opt(&self.cf(column), opts, iter_mode)
            .map(|item| {
                item.map(|(key, value)| {
                    let value_as_vec = value.to_vec();
                    let key_as_vec = key.to_vec();
                    #[cfg(feature = "metrics")]
                    {
                        DATABASE_METRICS.read_meter.inc();
                        DATABASE_METRICS
                            .bytes_read_meter
                            .inc_by(key_as_vec.len() as u64 + value_as_vec.len() as u64);
                    }
                    (key_as_vec, value_as_vec)
                })
                .map_err(|e| Error::DatabaseError(Box::new(e)))
            });

        if let Some(prefix) = prefix {
            let prefix = prefix.to_vec();
            // end iterating when we've gone outside the prefix
            Box::new(iter.take_while(move |item| {
                if let Ok((key, _)) = item {
                    key.starts_with(prefix.as_slice())
                } else {
                    true
                }
            }))
        } else {
            Box::new(iter)
        }
    }
}

impl BatchOperations for RocksDb {
    fn batch_write(&self, entries: &mut dyn Iterator<Item = WriteOperation>) -> Result<(), Error> {
        let mut batch = WriteBatch::default();

        for entry in entries {
            match entry {
                WriteOperation::Insert(key, column, value) => {
                    batch.put_cf(&self.cf(column), key, value);
                }
                WriteOperation::Remove(key, column) => {
                    batch.delete_cf(&self.cf(column), key);
                }
            }
        }
        #[cfg(feature = "metrics")]
        {
            DATABASE_METRICS.write_meter.inc();
            DATABASE_METRICS
                .bytes_written_meter
                .inc_by(batch.size_in_bytes() as u64);
        }
        self.db
            .write(batch)
            .map_err(|e| Error::DatabaseError(Box::new(e)))
    }
}

impl TransactableStorage for RocksDb {}

impl From<IterDirection> for rocksdb::Direction {
    fn from(d: IterDirection) -> Self {
        match d {
            IterDirection::Forward => rocksdb::Direction::Forward,
            IterDirection::Reverse => rocksdb::Direction::Reverse,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_db(cols: u32) -> (RocksDb, TempDir) {
        let tmp_dir = TempDir::new().unwrap();
        (RocksDb::open(tmp_dir.path(), cols).unwrap(), tmp_dir)
    }

    #[test]
    fn can_put_and_read() {
        let key = vec![0xA, 0xB, 0xC];

        let (db, _tmp) = create_db(1);
        db.put(key.clone(), 0, vec![1, 2, 3]).unwrap();

        assert_eq!(db.get(&key, 0).unwrap().unwrap(), vec![1, 2, 3])
    }

    #[test]
    fn put_returns_previous_value() {
        let key = vec![0xA, 0xB, 0xC];

        let (db, _tmp) = create_db(1);
        db.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        let prev = db.put(key, 0, vec![2, 4, 6]).unwrap();

        assert_eq!(prev, Some(vec![1, 2, 3]));
    }

    #[test]
    fn delete_and_get() {
        let key = vec![0xA, 0xB, 0xC];

        let (db, _tmp) = create_db(1);
        db.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        assert_eq!(db.get(&key, 0).unwrap().unwrap(), vec![1, 2, 3]);

        db.delete(&key, 0).unwrap();
        assert_eq!(db.get(&key, 0).unwrap(), None);
    }

    #[test]
    fn key_exists() {
        let key = vec![0xA, 0xB, 0xC];

        let (db, _tmp) = create_db(1);
        db.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        assert!(db.exists(&key, 0).unwrap());
    }

    #[test]
    fn batch_write_inserts() {
        let key = vec![0xA, 0xB, 0xC];
        let value = vec![1, 2, 3];

        let (db, _tmp) = create_db(1);
        let ops = vec![WriteOperation::Insert(key.clone(), 0, value.clone())];

        db.batch_write(&mut ops.into_iter()).unwrap();
        assert_eq!(db.get(&key, 0).unwrap().unwrap(), value)
    }

    #[test]
    fn batch_write_removes() {
        let key = vec![0xA, 0xB, 0xC];
        let value = vec![1, 2, 3];

        let (db, _tmp) = create_db(1);
        db.put(key.clone(), 0, value).unwrap();

        let ops = vec![WriteOperation::Remove(key.clone(), 0)];
        db.batch_write(&mut ops.into_iter()).unwrap();

        assert_eq!(db.get(&key, 0).unwrap(), None);
    }
}
