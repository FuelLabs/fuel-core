use crate::state::{
    BatchOperations, ColumnId, Error, KeyValueStore, TransactableStorage, WriteOperation,
};
use rocksdb::{
    BoundColumnFamily, ColumnFamilyDescriptor, DBWithThreadMode, IteratorMode, MultiThreaded,
    Options, WriteBatch,
};
use std::path::Path;
use std::sync::Arc;

type DB = DBWithThreadMode<MultiThreaded>;

#[derive(Debug)]
pub struct RocksDb {
    db: DBWithThreadMode<MultiThreaded>,
}

impl RocksDb {
    pub fn open<P: AsRef<Path>>(path: P, cols: u32) -> Result<RocksDb, Error> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let cf_descriptors: Vec<_> = (0..cols)
            .map(|i| ColumnFamilyDescriptor::new(RocksDb::col_name(i), opts.clone()))
            .collect();

        let db = match DB::open_cf_descriptors(&opts, &path, cf_descriptors) {
            Err(_) => {
                // setup cfs
                match DB::open_cf(&opts, &path, &[] as &[&str]) {
                    Ok(db) => {
                        for i in 0..cols {
                            let _ = db
                                .create_cf(RocksDb::col_name(i), &opts)
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

        Ok(RocksDb { db })
    }

    fn cf(&self, column: ColumnId) -> Arc<BoundColumnFamily> {
        self.db
            .cf_handle(&*RocksDb::col_name(column))
            .expect("invalid column state")
    }

    fn col_name(column: ColumnId) -> String {
        format!("column-{}", column)
    }
}

impl KeyValueStore for RocksDb {
    fn get(&self, key: &[u8], column: ColumnId) -> crate::state::Result<Option<Vec<u8>>> {
        self.db
            .get_cf(&self.cf(column), key)
            .map_err(|e| Error::DatabaseError(Box::new(e)))
    }

    fn put(
        &self,
        key: Vec<u8>,
        column: ColumnId,
        value: Vec<u8>,
    ) -> crate::state::Result<Option<Vec<u8>>> {
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
        Ok(self.db.key_may_exist_cf(&self.cf(column), key))
    }

    fn iter_all(&self, column: ColumnId) -> Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + '_> {
        Box::new(
            self.db
                .iterator_cf(&self.cf(column), IteratorMode::Start)
                .map(|(key, value)| (key.to_vec(), value.to_vec())),
        )
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

        self.db
            .write(batch)
            .map_err(|e| Error::DatabaseError(Box::new(e)))
    }
}

impl TransactableStorage for RocksDb {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::path::PathBuf;

    fn tmp_path() -> PathBuf {
        let mut path = temp_dir();
        path.push(rand::random::<u16>().to_string());
        path
    }

    fn create_db(cols: u32) -> RocksDb {
        RocksDb::open(tmp_path(), cols).unwrap()
    }

    #[test]
    fn can_put_and_read() {
        let key = vec![0xA, 0xB, 0xC];

        let db = create_db(1);
        db.put(key.clone(), 0, vec![1, 2, 3]).unwrap();

        assert_eq!(db.get(&key, 0).unwrap().unwrap(), vec![1, 2, 3])
    }

    #[test]
    fn put_returns_previous_value() {
        let key = vec![0xA, 0xB, 0xC];

        let db = create_db(1);
        db.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        let prev = db.put(key.clone(), 0, vec![2, 4, 6]).unwrap();

        assert_eq!(prev, Some(vec![1, 2, 3]));
    }

    #[test]
    fn delete_and_get() {
        let key = vec![0xA, 0xB, 0xC];

        let db = create_db(1);
        db.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        assert_eq!(db.get(&key, 0).unwrap().unwrap(), vec![1, 2, 3]);

        db.delete(&key, 0).unwrap();
        assert_eq!(db.get(&key, 0).unwrap(), None);
    }

    #[test]
    fn key_exists() {
        let key = vec![0xA, 0xB, 0xC];

        let db = create_db(1);
        db.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        assert!(db.exists(&key, 0).unwrap());
    }

    #[test]
    fn batch_write_inserts() {
        let key = vec![0xA, 0xB, 0xC];
        let value = vec![1, 2, 3];

        let db = create_db(1);
        let ops = vec![WriteOperation::Insert(key.clone(), 0, value.clone())];

        db.batch_write(&mut ops.into_iter()).unwrap();
        assert_eq!(db.get(&key, 0).unwrap().unwrap(), value)
    }

    #[test]
    fn batch_write_removes() {
        let key = vec![0xA, 0xB, 0xC];
        let value = vec![1, 2, 3];

        let db = create_db(1);
        db.put(key.clone(), 0, value.clone()).unwrap();

        let ops = vec![WriteOperation::Remove(key.clone(), 0)];
        db.batch_write(&mut ops.into_iter()).unwrap();

        assert_eq!(db.get(&key, 0).unwrap(), None);
    }
}
