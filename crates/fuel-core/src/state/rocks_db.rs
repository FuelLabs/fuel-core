use crate::{
    database::{
        convert_to_rocksdb_direction,
        database_description::DatabaseDescription,
        Error as DatabaseError,
        Result as DatabaseResult,
    },
    state::{
        IterDirection,
        TransactableStorage,
    },
};
use fuel_core_metrics::core_metrics::database_metrics;
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyValueInspect,
        StorageColumn,
        Value,
        WriteOperation,
    },
    transactional::Changes,
    Result as StorageResult,
};
use rand::RngCore;
use rocksdb::{
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
    cmp,
    env,
    fmt,
    fmt::{
        Debug,
        Formatter,
    },
    iter,
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};
use tempfile::TempDir;

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

type DropFn = Box<dyn FnOnce() + Send + Sync>;
#[derive(Default)]
struct DropResources {
    // move resources into this closure to have them dropped when db drops
    drop: Option<DropFn>,
}

impl fmt::Debug for DropResources {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DropResources")
    }
}

impl<F: 'static + FnOnce() + Send + Sync> From<F> for DropResources {
    fn from(closure: F) -> Self {
        Self {
            drop: Option::Some(Box::new(closure)),
        }
    }
}

impl Drop for DropResources {
    fn drop(&mut self) {
        if let Some(drop) = self.drop.take() {
            (drop)()
        }
    }
}

#[derive(Debug)]
pub struct RocksDb<Description> {
    db: DB,
    // used for RAII
    _drop: DropResources,
    _marker: core::marker::PhantomData<Description>,
}

impl<Description> RocksDb<Description>
where
    Description: DatabaseDescription,
{
    pub fn default_open_temp(capacity: Option<usize>) -> DatabaseResult<Self> {
        let tmp_dir = TempDir::new().unwrap();
        let path = tmp_dir.path();
        let result = Self::open(
            path,
            enum_iterator::all::<Description::Column>().collect::<Vec<_>>(),
            capacity,
        );
        let mut db = result?;

        db._drop = {
            move || {
                // cleanup temp dir
                drop(tmp_dir);
            }
        }
        .into();

        Ok(db)
    }

    pub fn default_open<P: AsRef<Path>>(
        path: P,
        capacity: Option<usize>,
    ) -> DatabaseResult<Self> {
        Self::open(
            path,
            enum_iterator::all::<Description::Column>().collect::<Vec<_>>(),
            capacity,
        )
    }

    pub fn prune(path: &Path) -> DatabaseResult<()> {
        let path = path.join(Description::name());
        DB::destroy(&Options::default(), path)
            .map_err(|e| DatabaseError::Other(e.into()))?;
        Ok(())
    }

    pub fn open<P: AsRef<Path>>(
        path: P,
        columns: Vec<Description::Column>,
        capacity: Option<usize>,
    ) -> DatabaseResult<Self> {
        Self::open_with(DB::open_cf_descriptors, path, columns, capacity)
    }

    pub fn open_read_only<P: AsRef<Path>>(
        path: P,
        columns: Vec<Description::Column>,
        capacity: Option<usize>,
        error_if_log_file_exist: bool,
    ) -> DatabaseResult<Self> {
        Self::open_with(
            |options, primary_path, cfs| {
                DB::open_cf_descriptors_read_only(
                    options,
                    primary_path,
                    cfs,
                    error_if_log_file_exist,
                )
            },
            path,
            columns,
            capacity,
        )
    }

    pub fn open_secondary<PrimaryPath, SecondaryPath>(
        path: PrimaryPath,
        secondary_path: SecondaryPath,
        columns: Vec<Description::Column>,
        capacity: Option<usize>,
    ) -> DatabaseResult<Self>
    where
        PrimaryPath: AsRef<Path>,
        SecondaryPath: AsRef<Path>,
    {
        Self::open_with(
            |options, primary_path, cfs| {
                DB::open_cf_descriptors_as_secondary(
                    options,
                    primary_path,
                    secondary_path.as_ref().to_path_buf(),
                    cfs,
                )
            },
            path,
            columns,
            capacity,
        )
    }

    pub fn open_with<F, P>(
        opener: F,
        path: P,
        columns: Vec<Description::Column>,
        capacity: Option<usize>,
    ) -> DatabaseResult<Self>
    where
        F: Fn(
            &Options,
            PathBuf,
            Vec<ColumnFamilyDescriptor>,
        ) -> Result<DB, rocksdb::Error>,
        P: AsRef<Path>,
    {
        let path = path.as_ref().join(Description::name());
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

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(DBCompressionType::Lz4);
        // TODO: Make it customizable https://github.com/FuelLabs/fuel-core/issues/1666
        opts.set_max_total_wal_size(64 * 1024 * 1024);
        let cpu_number =
            i32::try_from(num_cpus::get()).expect("The number of CPU can't exceed `i32`");
        opts.increase_parallelism(cmp::max(1, cpu_number / 2));
        if let Some(capacity) = capacity {
            // Set cache size 1/3 of the capacity. Another 1/3 is
            // used by block cache and the last 1 / 3 remains for other purposes:
            //
            // https://github.com/facebook/rocksdb/wiki/Setup-Options-and-Basic-Tuning#block-cache-size
            let row_cache_size = capacity / 3;
            let cache = Cache::new_lru_cache(row_cache_size);
            opts.set_row_cache(&cache);
        }

        let existing_column_families = DB::list_cf(&opts, &path).unwrap_or_default();

        let mut cf_descriptors_to_open = vec![];
        let mut cf_descriptors_to_create = vec![];
        for column in columns.clone() {
            let column_name = Self::col_name(column.id());
            let opts = Self::cf_opts(column, &block_opts);
            if existing_column_families.contains(&column_name) {
                cf_descriptors_to_open.push((column_name, opts));
            } else {
                cf_descriptors_to_create.push((column_name, opts));
            }
        }

        let iterator = cf_descriptors_to_open
            .clone()
            .into_iter()
            .map(|(name, opts)| ColumnFamilyDescriptor::new(name, opts))
            .collect::<Vec<_>>();

        let db = match opener(&opts, path.clone(), iterator) {
            Ok(db) => {
                Ok(db)
            },
            Err(err) => {
                tracing::error!("Couldn't open the database with an error: {}. \nTrying to repair the database", err);
                DB::repair(&opts, &path)
                    .map_err(|e| DatabaseError::Other(e.into()))?;

                let iterator = cf_descriptors_to_open
                    .clone()
                    .into_iter()
                    .map(|(name, opts)| ColumnFamilyDescriptor::new(name, opts))
                    .collect::<Vec<_>>();

                opener(&opts, path, iterator)
            },
        }
        .map_err(|e| DatabaseError::Other(e.into()))?;

        // Setup cfs
        for (name, opt) in cf_descriptors_to_create {
            db.create_cf(name, &opt)
                .map_err(|e| DatabaseError::Other(e.into()))?;
        }

        let rocks_db = RocksDb {
            db,
            _drop: Default::default(),
            _marker: Default::default(),
        };
        Ok(rocks_db)
    }

    fn cf(&self, column: Description::Column) -> Arc<BoundColumnFamily> {
        self.cf_u32(column.id())
    }

    fn cf_u32(&self, column: u32) -> Arc<BoundColumnFamily> {
        self.db
            .cf_handle(&Self::col_name(column))
            .expect("invalid column state")
    }

    fn col_name(column: u32) -> String {
        format!("col-{}", column)
    }

    fn cf_opts(column: Description::Column, block_opts: &BlockBasedOptions) -> Options {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(DBCompressionType::Lz4);
        opts.set_block_based_table_factory(block_opts);

        // All double-keys should be configured here
        if let Some(size) = Description::prefix(&column) {
            opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(size))
        }

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
        column: Description::Column,
    ) -> impl Iterator<Item = KVItem> + '_ {
        let maybe_next_item = next_prefix(prefix.to_vec())
            .and_then(|next_prefix| {
                self.iter_store(
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
        column: Description::Column,
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
                .map_err(|e| DatabaseError::Other(e.into()).into())
            })
    }
}

impl<Description> KeyValueInspect for RocksDb<Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        database_metrics().read_meter.inc();

        Ok(self
            .db
            .get_pinned_cf(&self.cf(column), key)
            .map_err(|e| DatabaseError::Other(e.into()))?
            .map(|value| value.len()))
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
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
        column: Self::Column,
        mut buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        database_metrics().read_meter.inc();

        let r = self
            .db
            .get_pinned_cf(&self.cf(column), key)
            .map_err(|e| DatabaseError::Other(e.into()))?
            .map(|value| {
                let read = value.len();
                std::io::Write::write_all(&mut buf, value.as_ref())
                    .map_err(|e| DatabaseError::Other(anyhow::anyhow!(e)))?;
                StorageResult::Ok(read)
            })
            .transpose()?;

        if let Some(r) = &r {
            database_metrics().bytes_read.observe(*r as f64);
        }

        Ok(r)
    }
}

impl<Description> IterableStore for RocksDb<Description>
where
    Description: DatabaseDescription,
{
    fn iter_store(
        &self,
        column: Self::Column,
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

                    // Setting prefix on the RocksDB level to optimize iteration.
                    let mut opts = ReadOptions::default();
                    opts.set_prefix_same_as_start(true);

                    let prefix = prefix.to_vec();
                    self._iter_all(column, opts, iter_mode)
                        // Not all tables has a prefix set, so we need to filter out the keys.
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
                    return iter::empty().into_boxed();
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

impl<Description> TransactableStorage<Description::Height> for RocksDb<Description>
where
    Description: DatabaseDescription,
{
    fn commit_changes(
        &self,
        _: Option<Description::Height>,
        changes: Changes,
    ) -> StorageResult<()> {
        let mut batch = WriteBatch::default();

        for (column, ops) in changes {
            let cf = self.cf_u32(column);
            for (key, op) in ops {
                match op {
                    WriteOperation::Insert(value) => {
                        batch.put_cf(&cf, key, value.as_ref());
                    }
                    WriteOperation::Remove => {
                        batch.delete_cf(&cf, key);
                    }
                }
            }
        }

        database_metrics().write_meter.inc();
        database_metrics()
            .bytes_written
            .observe(batch.size_in_bytes() as f64);

        self.db
            .write(batch)
            .map_err(|e| DatabaseError::Other(e.into()).into())
    }
}

/// The `None` means overflow, so there is not following prefix.
fn next_prefix(mut prefix: Vec<u8>) -> Option<Vec<u8>> {
    for byte in prefix.iter_mut().rev() {
        if let Some(new_byte) = byte.checked_add(1) {
            *byte = new_byte;
            return Some(prefix);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::database_description::on_chain::OnChain;
    use fuel_core_storage::{
        column::Column,
        kv_store::KeyValueMutate,
        transactional::ReadTransaction,
    };
    use std::collections::{
        BTreeMap,
        HashMap,
    };
    use tempfile::TempDir;

    impl<Description> KeyValueMutate for RocksDb<Description>
    where
        Description: DatabaseDescription,
    {
        fn write(
            &mut self,
            key: &[u8],
            column: Self::Column,
            buf: &[u8],
        ) -> StorageResult<usize> {
            let mut transaction = self.read_transaction();
            let len = transaction.write(key, column, buf)?;
            let changes = transaction.into_changes();
            self.commit_changes(None, changes)?;

            Ok(len)
        }

        fn delete(&mut self, key: &[u8], column: Self::Column) -> StorageResult<()> {
            let mut transaction = self.read_transaction();
            transaction.delete(key, column)?;
            let changes = transaction.into_changes();
            self.commit_changes(None, changes)?;
            Ok(())
        }
    }

    fn create_db() -> (RocksDb<OnChain>, TempDir) {
        let tmp_dir = TempDir::new().unwrap();
        (
            RocksDb::default_open(tmp_dir.path(), None).unwrap(),
            tmp_dir,
        )
    }

    #[test]
    fn open_new_columns() {
        let tmp_dir = TempDir::new().unwrap();

        // Given
        let old_columns =
            vec![Column::Coins, Column::Messages, Column::UploadedBytecodes];
        let database_with_old_columns =
            RocksDb::<OnChain>::open(tmp_dir.path(), old_columns.clone(), None)
                .expect("Failed to open database with old columns");
        drop(database_with_old_columns);

        // When
        let mut new_columns = old_columns;
        new_columns.push(Column::ContractsAssets);
        new_columns.push(Column::Metadata);
        let database_with_new_columns =
            RocksDb::<OnChain>::open(tmp_dir.path(), new_columns, None).map(|_| ());

        // Then
        assert_eq!(Ok(()), database_with_new_columns);
    }

    #[test]
    fn can_put_and_read() {
        let key = vec![0xA, 0xB, 0xC];

        let (mut db, _tmp) = create_db();
        let expected = Arc::new(vec![1, 2, 3]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected)
    }

    #[test]
    fn put_returns_previous_value() {
        let key = vec![0xA, 0xB, 0xC];

        let (mut db, _tmp) = create_db();
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

        let (mut db, _tmp) = create_db();
        let expected = Arc::new(vec![1, 2, 3]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();
        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected);

        db.delete(&key, Column::Metadata).unwrap();
        assert_eq!(db.get(&key, Column::Metadata).unwrap(), None);
    }

    #[test]
    fn key_exists() {
        let key = vec![0xA, 0xB, 0xC];

        let (mut db, _tmp) = create_db();
        let expected = Arc::new(vec![1, 2, 3]);
        db.put(&key, Column::Metadata, expected).unwrap();
        assert!(db.exists(&key, Column::Metadata).unwrap());
    }

    #[test]
    fn commit_changes_inserts() {
        let key = vec![0xA, 0xB, 0xC];
        let value = Arc::new(vec![1, 2, 3]);

        let (db, _tmp) = create_db();
        let ops = vec![(
            Column::Metadata.id(),
            BTreeMap::from_iter(vec![(
                key.clone().into(),
                WriteOperation::Insert(value.clone()),
            )]),
        )];

        db.commit_changes(None, HashMap::from_iter(ops)).unwrap();
        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), value)
    }

    #[test]
    fn commit_changes_removes() {
        let key = vec![0xA, 0xB, 0xC];
        let value = Arc::new(vec![1, 2, 3]);

        let (mut db, _tmp) = create_db();
        db.put(&key, Column::Metadata, value).unwrap();

        let ops = vec![(
            Column::Metadata.id(),
            BTreeMap::from_iter(vec![(key.clone().into(), WriteOperation::Remove)]),
        )];
        db.commit_changes(None, HashMap::from_iter(ops)).unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap(), None);
    }

    #[test]
    fn can_use_unit_value() {
        let key = vec![0x00];

        let (mut db, _tmp) = create_db();
        let expected = Arc::new(vec![]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(db.exists(&key, Column::Metadata).unwrap());

        assert_eq!(
            db.iter_store(Column::Metadata, None, None, IterDirection::Forward)
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

        let (mut db, _tmp) = create_db();
        let expected = Arc::new(vec![1, 2, 3]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(db.exists(&key, Column::Metadata).unwrap());

        assert_eq!(
            db.iter_store(Column::Metadata, None, None, IterDirection::Forward)
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

        let (mut db, _tmp) = create_db();
        let expected = Arc::new(vec![]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(db.exists(&key, Column::Metadata).unwrap());

        assert_eq!(
            db.iter_store(Column::Metadata, None, None, IterDirection::Forward)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()[0],
            (key.clone(), expected.clone())
        );

        assert_eq!(db.take(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(!db.exists(&key, Column::Metadata).unwrap());
    }

    #[test]
    fn open_primary_db_second_time_fails() {
        // Given
        let (_primary_db, tmp_dir) = create_db();

        // When
        let old_columns =
            vec![Column::Coins, Column::Messages, Column::UploadedBytecodes];
        let result = RocksDb::<OnChain>::open(tmp_dir.path(), old_columns.clone(), None);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn open_second_read_only_db() {
        // Given
        let (_primary_db, tmp_dir) = create_db();

        // When
        let old_columns =
            vec![Column::Coins, Column::Messages, Column::UploadedBytecodes];
        let result = RocksDb::<OnChain>::open_read_only(
            tmp_dir.path(),
            old_columns.clone(),
            None,
            false,
        )
        .map(|_| ());

        // Then
        assert_eq!(Ok(()), result);
    }

    #[test]
    fn open_secondary_db() {
        // Given
        let (_primary_db, tmp_dir) = create_db();
        let secondary_temp = TempDir::new().unwrap();

        // When
        let old_columns =
            vec![Column::Coins, Column::Messages, Column::UploadedBytecodes];
        let result = RocksDb::<OnChain>::open_secondary(
            tmp_dir.path(),
            secondary_temp.path(),
            old_columns.clone(),
            None,
        )
        .map(|_| ());

        // Then
        assert_eq!(Ok(()), result);
    }
}
