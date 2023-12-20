use crate::{
    database::{
        convert_to_rocksdb_direction,
        Column,
        Database,
        Error as DatabaseError,
        Result as DatabaseResult,
    },
    state::{
        BatchOperations,
        IterDirection,
        KVItem,
        KeyValueStore,
        TransactableStorage,
        Value,
        WriteOperation,
    },
};
use fuel_core_metrics::core_metrics::database_metrics;
use fuel_core_storage::iter::{
    BoxedIter,
    IntoBoxedIter,
};
use rand::RngCore;
use rocksdb::{
    checkpoint::Checkpoint,
    BlockBasedOptions,
    BoundColumnFamily,
    Cache,
    ColumnFamilyDescriptor,
    DBCompressionType,
    DBWithThreadMode,
    IteratorMode,
    MultiThreaded,
    Options,
    ReadOptions,
    SliceTransform,
    WriteBatch,
};
use std::{
    env,
    iter,
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};

type DB = DBWithThreadMode<MultiThreaded>;

/// Reimplementation of `tempdir::TempDir` that allows creating a new
/// instance without actually creating a new directory on the filesystem.
/// This is needed since rocksdb requires empty directory for checkpoints.
pub struct ShallowTempDir {
    path: PathBuf,
}

impl Default for ShallowTempDir {
    fn default() -> Self {
        Self::new()
    }
}

impl ShallowTempDir {
    /// Creates a random directory.
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let mut path = env::temp_dir();
        path.push(format!("fuel-core-shallow-{}", rng.next_u64()));
        Self { path }
    }

    /// Returns the path of the directory.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for ShallowTempDir {
    fn drop(&mut self) {
        // Ignore errors
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[derive(Debug)]
pub struct RocksDb {
    db: DB,
    capacity: Option<usize>,
}

impl RocksDb {
    pub fn default_open<P: AsRef<Path>>(
        path: P,
        capacity: Option<usize>,
    ) -> DatabaseResult<RocksDb> {
        Self::open(
            path,
            enum_iterator::all::<Column>().collect::<Vec<_>>(),
            capacity,
        )
    }

    pub fn open<P: AsRef<Path>>(
        path: P,
        columns: Vec<Column>,
        capacity: Option<usize>,
    ) -> DatabaseResult<RocksDb> {
        let mut block_opts = BlockBasedOptions::default();
        // See https://github.com/facebook/rocksdb/blob/a1523efcdf2f0e8133b9a9f6e170a0dad49f928f/include/rocksdb/table.h#L246-L271 for details on what the format versions are/do.
        block_opts.set_format_version(5);

        if let Some(capacity) = capacity {
            // Set cache size 1/3 of the capacity as recommended by
            // https://github.com/facebook/rocksdb/wiki/Setup-Options-and-Basic-Tuning#block-cache-size
            let block_cache_size = capacity / 3;
            let cache = Cache::new_lru_cache(block_cache_size);
            block_opts.set_block_cache(&cache);
            // "index and filter blocks will be stored in block cache, together with all other data blocks."
            // See: https://github.com/facebook/rocksdb/wiki/Memory-usage-in-RocksDB#indexes-and-filter-blocks
            block_opts.set_cache_index_and_filter_blocks(true);
            // Don't evict L0 filter/index blocks from the cache
            block_opts.set_pin_l0_filter_and_index_blocks_in_cache(true);
        } else {
            block_opts.disable_cache();
        }
        block_opts.set_bloom_filter(10.0, true);

        let cf_descriptors = columns.clone().into_iter().map(|i| {
            ColumnFamilyDescriptor::new(
                RocksDb::col_name(i),
                Self::cf_opts(i, &block_opts),
            )
        });

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(DBCompressionType::Lz4);
        if let Some(capacity) = capacity {
            // Set cache size 1/3 of the capacity. Another 1/3 is
            // used by block cache and the last 1 / 3 remains for other purposes:
            //
            // https://github.com/facebook/rocksdb/wiki/Setup-Options-and-Basic-Tuning#block-cache-size
            let row_cache_size = capacity / 3;
            let cache = Cache::new_lru_cache(row_cache_size);
            opts.set_row_cache(&cache);
        }

        let db = match DB::open_cf_descriptors(&opts, &path, cf_descriptors) {
            Err(_) => {
                // setup cfs
                match DB::open_cf(&opts, &path, &[] as &[&str]) {
                    Ok(db) => {
                        for i in columns {
                            let opts = Self::cf_opts(i, &block_opts);
                            db.create_cf(RocksDb::col_name(i), &opts)
                                .map_err(|e| DatabaseError::Other(e.into()))?;
                        }
                        Ok(db)
                    }
                    Err(err) => {
                        tracing::error!("Couldn't open the database with an error: {}. \nTrying to repair the database", err);
                        DB::repair(&opts, &path)
                            .map_err(|e| DatabaseError::Other(e.into()))?;

                        let cf_descriptors = columns.clone().into_iter().map(|i| {
                            ColumnFamilyDescriptor::new(
                                RocksDb::col_name(i),
                                Self::cf_opts(i, &block_opts),
                            )
                        });
                        DB::open_cf_descriptors(&opts, &path, cf_descriptors)
                    }
                }
            }
            ok => ok,
        }
        .map_err(|e| DatabaseError::Other(e.into()))?;
        let rocks_db = RocksDb { db, capacity };
        Ok(rocks_db)
    }

    pub fn checkpoint<P: AsRef<Path>>(&self, path: P) -> DatabaseResult<()> {
        Checkpoint::new(&self.db)
            .and_then(|checkpoint| checkpoint.create_checkpoint(path))
            .map_err(|e| {
                DatabaseError::Other(anyhow::anyhow!(
                    "Failed to create a checkpoint: {}",
                    e
                ))
            })
    }

    fn cf(&self, column: Column) -> Arc<BoundColumnFamily> {
        self.db
            .cf_handle(&RocksDb::col_name(column))
            .expect("invalid column state")
    }

    fn col_name(column: Column) -> String {
        format!("col-{}", column.as_usize())
    }

    fn cf_opts(column: Column, block_opts: &BlockBasedOptions) -> Options {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(DBCompressionType::Lz4);
        opts.set_block_based_table_factory(block_opts);

        // All double-keys should be configured here
        match column {
            Column::OwnedCoins
            | Column::TransactionsByOwnerBlockIdx
            | Column::OwnedMessageIds
            | Column::ContractsAssets
            | Column::ContractsState => {
                // prefix is address length
                opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(32))
            }
            _ => {}
        };

        opts
    }

    /// RocksDB prefix iteration doesn't support reverse order,
    /// but seeking the start key and iterating in reverse order works.
    /// So we can create a workaround. We need to find the next available
    /// element and use it as an anchor for reverse iteration,
    /// but skip the first element to jump on the previous prefix.
    /// If we can't find the next element, we are at the end of the list,
    /// so we can use `IteratorMode::End` to start reverse iteration.
    fn reverse_prefix_iter(
        &self,
        prefix: &[u8],
        column: Column,
    ) -> impl Iterator<Item = KVItem> + '_ {
        let maybe_next_item = next_prefix(prefix.to_vec())
            .and_then(|next_prefix| {
                self.iter_all(
                    column,
                    Some(next_prefix.as_slice()),
                    None,
                    IterDirection::Forward,
                )
                .next()
            })
            .and_then(|res| res.ok());

        if let Some((next_start_key, _)) = maybe_next_item {
            let iter_mode = IteratorMode::From(
                next_start_key.as_slice(),
                rocksdb::Direction::Reverse,
            );
            let prefix = prefix.to_vec();
            self
                ._iter_all(column, ReadOptions::default(), iter_mode)
                // Skip the element under the `next_start_key` key.
                .skip(1)
                .take_while(move |item| {
                    if let Ok((key, _)) = item {
                        key.starts_with(prefix.as_slice())
                    } else {
                        true
                    }
                })
                .into_boxed()
        } else {
            // No next item, so we can start backward iteration from the end.
            let prefix = prefix.to_vec();
            self._iter_all(column, ReadOptions::default(), IteratorMode::End)
                .take_while(move |item| {
                    if let Ok((key, _)) = item {
                        key.starts_with(prefix.as_slice())
                    } else {
                        true
                    }
                })
                .into_boxed()
        }
    }

    fn _iter_all(
        &self,
        column: Column,
        opts: ReadOptions,
        iter_mode: IteratorMode,
    ) -> impl Iterator<Item = KVItem> + '_ {
        self.db
            .iterator_cf_opt(&self.cf(column), opts, iter_mode)
            .map(|item| {
                item.map(|(key, value)| {
                    let value_as_vec = Vec::from(value);
                    let key_as_vec = Vec::from(key);

                    database_metrics().read_meter.inc();
                    database_metrics().bytes_read.observe(
                        (key_as_vec.len().saturating_add(value_as_vec.len())) as f64,
                    );

                    (key_as_vec, Arc::new(value_as_vec))
                })
                .map_err(|e| DatabaseError::Other(e.into()))
            })
    }
}

impl KeyValueStore for RocksDb {
    type Column = Column;

    fn write(&self, key: &[u8], column: Column, buf: &[u8]) -> DatabaseResult<usize> {
        let r = buf.len();
        self.db
            .put_cf(&self.cf(column), key, buf)
            .map_err(|e| DatabaseError::Other(e.into()))?;

        database_metrics().write_meter.inc();
        database_metrics().bytes_written.observe(r as f64);

        Ok(r)
    }

    fn delete(&self, key: &[u8], column: Column) -> DatabaseResult<()> {
        self.db
            .delete_cf(&self.cf(column), key)
            .map_err(|e| DatabaseError::Other(e.into()))
    }

    fn size_of_value(&self, key: &[u8], column: Column) -> DatabaseResult<Option<usize>> {
        database_metrics().read_meter.inc();

        Ok(self
            .db
            .get_pinned_cf(&self.cf(column), key)
            .map_err(|e| DatabaseError::Other(e.into()))?
            .map(|value| value.len()))
    }

    fn get(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>> {
        database_metrics().read_meter.inc();

        let value = self
            .db
            .get_cf(&self.cf(column), key)
            .map_err(|e| DatabaseError::Other(e.into()))?;

        if let Some(value) = &value {
            database_metrics().bytes_read.observe(value.len() as f64);
        }

        Ok(value.map(Arc::new))
    }

    fn read(
        &self,
        key: &[u8],
        column: Column,
        mut buf: &mut [u8],
    ) -> DatabaseResult<Option<usize>> {
        database_metrics().read_meter.inc();

        let r = self
            .db
            .get_pinned_cf(&self.cf(column), key)
            .map_err(|e| DatabaseError::Other(e.into()))?
            .map(|value| {
                let read = value.len();
                std::io::Write::write_all(&mut buf, value.as_ref())
                    .map_err(|e| DatabaseError::Other(anyhow::anyhow!(e)))?;
                DatabaseResult::Ok(read)
            })
            .transpose()?;

        if let Some(r) = &r {
            database_metrics().bytes_read.observe(*r as f64);
        }

        Ok(r)
    }

    fn iter_all(
        &self,
        column: Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        match (prefix, start) {
            (None, None) => {
                let iter_mode =
                    // if no start or prefix just start iterating over entire keyspace
                    match direction {
                        IterDirection::Forward => IteratorMode::Start,
                        // end always iterates in reverse
                        IterDirection::Reverse => IteratorMode::End,
                    };
                self._iter_all(column, ReadOptions::default(), iter_mode)
                    .into_boxed()
            }
            (Some(prefix), None) => {
                if direction == IterDirection::Reverse {
                    self.reverse_prefix_iter(prefix, column).into_boxed()
                } else {
                    // start iterating in a certain direction within the keyspace
                    let iter_mode = IteratorMode::From(
                        prefix,
                        convert_to_rocksdb_direction(direction),
                    );
                    let mut opts = ReadOptions::default();
                    opts.set_prefix_same_as_start(true);

                    self._iter_all(column, opts, iter_mode).into_boxed()
                }
            }
            (None, Some(start)) => {
                // start iterating in a certain direction from the start key
                let iter_mode =
                    IteratorMode::From(start, convert_to_rocksdb_direction(direction));
                self._iter_all(column, ReadOptions::default(), iter_mode)
                    .into_boxed()
            }
            (Some(prefix), Some(start)) => {
                // TODO: Maybe we want to allow the `start` to be without a `prefix` in the future.
                // If the `start` doesn't have the same `prefix`, return nothing.
                if !start.starts_with(prefix) {
                    return iter::empty().into_boxed()
                }

                // start iterating in a certain direction from the start key
                // and end iterating when we've gone outside the prefix
                let prefix = prefix.to_vec();
                let iter_mode =
                    IteratorMode::From(start, convert_to_rocksdb_direction(direction));
                self._iter_all(column, ReadOptions::default(), iter_mode)
                    .take_while(move |item| {
                        if let Ok((key, _)) = item {
                            key.starts_with(prefix.as_slice())
                        } else {
                            true
                        }
                    })
                    .into_boxed()
            }
        }
    }
}

impl BatchOperations for RocksDb {
    fn batch_write(
        &self,
        entries: &mut dyn Iterator<Item = (Vec<u8>, Column, WriteOperation)>,
    ) -> DatabaseResult<()> {
        let mut batch = WriteBatch::default();

        for (key, column, op) in entries {
            match op {
                WriteOperation::Insert(value) => {
                    batch.put_cf(&self.cf(column), key, value.as_ref());
                }
                WriteOperation::Remove => {
                    batch.delete_cf(&self.cf(column), key);
                }
            }
        }

        database_metrics().write_meter.inc();
        database_metrics()
            .bytes_written
            .observe(batch.size_in_bytes() as f64);

        self.db
            .write(batch)
            .map_err(|e| DatabaseError::Other(e.into()))
    }
}

impl TransactableStorage for RocksDb {
    fn checkpoint(&self) -> DatabaseResult<Database> {
        let tmp_dir = ShallowTempDir::new();
        self.checkpoint(&tmp_dir.path)?;
        let db = RocksDb::default_open(&tmp_dir.path, self.capacity)?;
        let database = Database::new(Arc::new(db)).with_drop(Box::new(move || {
            drop(tmp_dir);
        }));

        Ok(database)
    }

    fn flush(&self) -> DatabaseResult<()> {
        self.db
            .flush_wal(true)
            .map_err(|e| anyhow::anyhow!("Unable to flush WAL file: {}", e))?;
        self.db
            .flush()
            .map_err(|e| anyhow::anyhow!("Unable to flush SST files: {}", e))?;
        Ok(())
    }
}

/// The `None` means overflow, so there is not following prefix.
fn next_prefix(mut prefix: Vec<u8>) -> Option<Vec<u8>> {
    for byte in prefix.iter_mut().rev() {
        if let Some(new_byte) = byte.checked_add(1) {
            *byte = new_byte;
            return Some(prefix)
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_db() -> (RocksDb, TempDir) {
        let tmp_dir = TempDir::new().unwrap();
        (
            RocksDb::default_open(tmp_dir.path(), None).unwrap(),
            tmp_dir,
        )
    }

    #[test]
    fn can_put_and_read() {
        let key = vec![0xA, 0xB, 0xC];

        let (db, _tmp) = create_db();
        let expected = Arc::new(vec![1, 2, 3]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected)
    }

    #[test]
    fn put_returns_previous_value() {
        let key = vec![0xA, 0xB, 0xC];

        let (db, _tmp) = create_db();
        let expected = Arc::new(vec![1, 2, 3]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();
        let prev = db
            .replace(&key, Column::Metadata, Arc::new(vec![2, 4, 6]))
            .unwrap();

        assert_eq!(prev, Some(expected));
    }

    #[test]
    fn delete_and_get() {
        let key = vec![0xA, 0xB, 0xC];

        let (db, _tmp) = create_db();
        let expected = Arc::new(vec![1, 2, 3]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();
        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected);

        db.delete(&key, Column::Metadata).unwrap();
        assert_eq!(db.get(&key, Column::Metadata).unwrap(), None);
    }

    #[test]
    fn key_exists() {
        let key = vec![0xA, 0xB, 0xC];

        let (db, _tmp) = create_db();
        let expected = Arc::new(vec![1, 2, 3]);
        db.put(&key, Column::Metadata, expected).unwrap();
        assert!(db.exists(&key, Column::Metadata).unwrap());
    }

    #[test]
    fn batch_write_inserts() {
        let key = vec![0xA, 0xB, 0xC];
        let value = Arc::new(vec![1, 2, 3]);

        let (db, _tmp) = create_db();
        let ops = vec![(
            key.clone(),
            Column::Metadata,
            WriteOperation::Insert(value.clone()),
        )];

        db.batch_write(&mut ops.into_iter()).unwrap();
        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), value)
    }

    #[test]
    fn batch_write_removes() {
        let key = vec![0xA, 0xB, 0xC];
        let value = Arc::new(vec![1, 2, 3]);

        let (db, _tmp) = create_db();
        db.put(&key, Column::Metadata, value).unwrap();

        let ops = vec![(key.clone(), Column::Metadata, WriteOperation::Remove)];
        db.batch_write(&mut ops.into_iter()).unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap(), None);
    }

    #[test]
    fn can_use_unit_value() {
        let key = vec![0x00];

        let (db, _tmp) = create_db();
        let expected = Arc::new(vec![]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(db.exists(&key, Column::Metadata).unwrap());

        assert_eq!(
            db.iter_all(Column::Metadata, None, None, IterDirection::Forward)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()[0],
            (key.clone(), expected.clone())
        );

        assert_eq!(db.take(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(!db.exists(&key, Column::Metadata).unwrap());
    }

    #[test]
    fn can_use_unit_key() {
        let key: Vec<u8> = Vec::with_capacity(0);

        let (db, _tmp) = create_db();
        let expected = Arc::new(vec![1, 2, 3]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(db.exists(&key, Column::Metadata).unwrap());

        assert_eq!(
            db.iter_all(Column::Metadata, None, None, IterDirection::Forward)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()[0],
            (key.clone(), expected.clone())
        );

        assert_eq!(db.take(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(!db.exists(&key, Column::Metadata).unwrap());
    }

    #[test]
    fn can_use_unit_key_and_value() {
        let key: Vec<u8> = Vec::with_capacity(0);

        let (db, _tmp) = create_db();
        let expected = Arc::new(vec![]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(db.exists(&key, Column::Metadata).unwrap());

        assert_eq!(
            db.iter_all(Column::Metadata, None, None, IterDirection::Forward)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()[0],
            (key.clone(), expected.clone())
        );

        assert_eq!(db.take(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(!db.exists(&key, Column::Metadata).unwrap());
    }
}
