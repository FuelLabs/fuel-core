use crate::{
    database::{
        convert_to_rocksdb_direction,
        database_description::DatabaseDescription,
        Error as DatabaseError,
        Result as DatabaseResult,
    },
    state::IterDirection,
};

use super::rocks_db_key_iterator::{
    ExtractItem,
    RocksDBKeyIterator,
};
use fuel_core_metrics::core_metrics::DatabaseMetrics;
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyItem,
        KeyValueInspect,
        StorageColumn,
        Value,
        WriteOperation,
    },
    transactional::Changes,
    Result as StorageResult,
};
use itertools::Itertools;
use rocksdb::{
    BlockBasedOptions,
    BoundColumnFamily,
    Cache,
    ColumnFamilyDescriptor,
    DBAccess,
    DBCompressionType,
    DBRawIteratorWithThreadMode,
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
    collections::BTreeMap,
    fmt,
    fmt::Formatter,
    iter,
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};
use tempfile::TempDir;

type DB = DBWithThreadMode<MultiThreaded>;

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

pub struct RocksDb<Description> {
    read_options: ReadOptions,
    db: Arc<DB>,
    snapshot: Option<rocksdb::SnapshotWithThreadMode<'static, DB>>,
    metrics: Arc<DatabaseMetrics>,
    // used for RAII
    _drop: Arc<DropResources>,
    _marker: core::marker::PhantomData<Description>,
}

impl<Description> Drop for RocksDb<Description> {
    fn drop(&mut self) {
        // Drop the snapshot before the db.
        // Dropping the snapshot after the db will cause a sigsegv.
        self.snapshot = None;
    }
}

impl<Description> std::fmt::Debug for RocksDb<Description> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RocksDb").field("db", &self.db).finish()
    }
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

        db._drop = Arc::new(
            {
                move || {
                    // cleanup temp dir
                    drop(tmp_dir);
                }
            }
            .into(),
        );

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
        let original_path = path.as_ref().to_path_buf();
        let path = original_path.join(Description::name());
        let metric_columns = columns
            .iter()
            .map(|column| (column.id(), column.name()))
            .collect::<Vec<_>>();
        let metrics = Arc::new(DatabaseMetrics::new(
            Description::name().as_str(),
            &metric_columns,
        ));
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
        block_opts.set_block_size(16 * 1024);

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
        opts.set_max_background_jobs(6);
        opts.set_bytes_per_sync(1048576);

        #[cfg(feature = "test-helpers")]
        opts.set_max_open_files(512);

        let existing_column_families = DB::list_cf(&opts, &path).unwrap_or_default();

        let mut cf_descriptors_to_open = BTreeMap::new();
        let mut cf_descriptors_to_create = BTreeMap::new();
        for column in columns.clone() {
            let column_name = Self::col_name(column.id());
            let opts = Self::cf_opts(column, &block_opts);
            if existing_column_families.contains(&column_name) {
                cf_descriptors_to_open.insert(column_name, opts);
            } else {
                cf_descriptors_to_create.insert(column_name, opts);
            }
        }

        let unknown_columns_to_open: BTreeMap<_, _> = existing_column_families
            .iter()
            .filter(|column_name| {
                !cf_descriptors_to_open.contains_key(*column_name)
                    && !cf_descriptors_to_create.contains_key(*column_name)
            })
            .map(|unknown_column_name| {
                let unknown_column_options = Self::default_opts(&block_opts);
                (unknown_column_name.clone(), unknown_column_options)
            })
            .collect();
        cf_descriptors_to_open.extend(unknown_columns_to_open);

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
                tracing::error!("Couldn't open the database with an error: {}. \nTrying to reopen the database", err);

                let iterator = cf_descriptors_to_open
                    .clone()
                    .into_iter()
                    .map(|(name, opts)| ColumnFamilyDescriptor::new(name, opts))
                    .collect::<Vec<_>>();

                opener(&opts, path.clone(), iterator)
            },
        }
        .map_err(|e| DatabaseError::Other(e.into()))?;

        // Setup cfs
        for (name, opt) in cf_descriptors_to_create {
            db.create_cf(name, &opt)
                .map_err(|e| DatabaseError::Other(e.into()))?;
        }

        let db = Arc::new(db);

        let rocks_db = RocksDb {
            read_options: Self::generate_read_options(&None),
            snapshot: None,
            db,
            metrics,
            _drop: Default::default(),
            _marker: Default::default(),
        };
        Ok(rocks_db)
    }

    fn generate_read_options(
        snapshot: &Option<rocksdb::SnapshotWithThreadMode<DB>>,
    ) -> ReadOptions {
        let mut opts = ReadOptions::default();
        opts.set_verify_checksums(false);
        if let Some(snapshot) = &snapshot {
            opts.set_snapshot(snapshot);
        }
        opts
    }

    fn read_options(&self) -> ReadOptions {
        Self::generate_read_options(&self.snapshot)
    }

    pub fn create_snapshot(&self) -> Self {
        self.create_snapshot_generic()
    }

    pub fn create_snapshot_generic<TargetDescription>(
        &self,
    ) -> RocksDb<TargetDescription> {
        let db = self.db.clone();
        let metrics = self.metrics.clone();
        let _drop = self._drop.clone();

        // Safety: We are transmuting the snapshot to 'static lifetime, but it's safe
        // because we are not going to use it after the RocksDb is dropped.
        // We control the lifetime of the `Self` - RocksDb, so we can guarantee that
        // the snapshot will be dropped before the RocksDb.
        #[allow(clippy::missing_transmute_annotations)]
        // Remove this and see for yourself
        let snapshot = unsafe {
            let snapshot = db.snapshot();
            core::mem::transmute(snapshot)
        };
        let snapshot = Some(snapshot);

        RocksDb {
            read_options: Self::generate_read_options(&snapshot),
            snapshot,
            db,
            metrics,
            _drop,
            _marker: Default::default(),
        }
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

    fn default_opts(block_opts: &BlockBasedOptions) -> Options {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(DBCompressionType::Lz4);
        opts.set_block_based_table_factory(block_opts);

        opts
    }

    fn cf_opts(column: Description::Column, block_opts: &BlockBasedOptions) -> Options {
        let mut opts = Self::default_opts(block_opts);

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
    fn reverse_prefix_iter<T>(
        &self,
        prefix: &[u8],
        column: Description::Column,
    ) -> impl Iterator<Item = StorageResult<T::Item>> + '_
    where
        T: ExtractItem,
    {
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
                .iterator::<T>(column, self.read_options(), iter_mode)
                // Skip the element under the `next_start_key` key.
                .skip(1)
                .take_while(move |item| {
                    if let Ok(item) = item {
                        T::starts_with(item, prefix.as_slice())
                    } else {
                        true
                    }
                })
                .into_boxed()
        } else {
            // No next item, so we can start backward iteration from the end.
            let prefix = prefix.to_vec();
            self.iterator::<T>(column, self.read_options(), IteratorMode::End)
                .take_while(move |item| {
                    if let Ok(item) = item {
                        T::starts_with(item, prefix.as_slice())
                    } else {
                        true
                    }
                })
                .into_boxed()
        }
    }

    pub(crate) fn iterator<T>(
        &self,
        column: Description::Column,
        opts: ReadOptions,
        iter_mode: IteratorMode,
    ) -> impl Iterator<Item = StorageResult<T::Item>> + '_
    where
        T: ExtractItem,
    {
        let column_metrics = self.metrics.columns_read_statistic.get(&column.id());

        RocksDBKeyIterator::<_, T>::new(
            self.db.raw_iterator_cf_opt(&self.cf(column), opts),
            iter_mode,
        )
        .map(move |item| {
            item.map(|item| {
                self.metrics.read_meter.inc();
                column_metrics.map(|metric| metric.inc());
                self.metrics.bytes_read.inc_by(T::size(&item));

                item
            })
            .map_err(|e| DatabaseError::Other(e.into()).into())
        })
    }

    pub fn multi_get<K, I>(
        &self,
        column: u32,
        iterator: I,
    ) -> DatabaseResult<Vec<Option<Vec<u8>>>>
    where
        I: Iterator<Item = K>,
        K: AsRef<[u8]>,
    {
        let column_metrics = self.metrics.columns_read_statistic.get(&column);
        let cl = self.cf_u32(column);
        let results = self
            .db
            .multi_get_cf_opt(iterator.map(|k| (&cl, k)), &self.read_options)
            .into_iter()
            .map(|el| {
                self.metrics.read_meter.inc();
                column_metrics.map(|metric| metric.inc());
                el.map(|value| {
                    value.map(|vec| {
                        self.metrics.bytes_read.inc_by(vec.len() as u64);
                        vec
                    })
                })
                .map_err(|err| DatabaseError::Other(err.into()))
            })
            .try_collect()?;
        Ok(results)
    }

    fn _iter_store<T>(
        &self,
        column: Description::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<T::Item>>
    where
        T: ExtractItem,
    {
        match (prefix, start) {
            (None, None) => {
                let iter_mode =
                    // if no start or prefix just start iterating over entire keyspace
                    match direction {
                        IterDirection::Forward => IteratorMode::Start,
                        // end always iterates in reverse
                        IterDirection::Reverse => IteratorMode::End,
                    };
                self.iterator::<T>(column, self.read_options(), iter_mode)
                    .into_boxed()
            }
            (Some(prefix), None) => {
                if direction == IterDirection::Reverse {
                    self.reverse_prefix_iter::<T>(prefix, column).into_boxed()
                } else {
                    // start iterating in a certain direction within the keyspace
                    let iter_mode = IteratorMode::From(
                        prefix,
                        convert_to_rocksdb_direction(direction),
                    );

                    // Setting prefix on the RocksDB level to optimize iteration.
                    let mut opts = self.read_options();
                    opts.set_prefix_same_as_start(true);

                    let prefix = prefix.to_vec();
                    self.iterator::<T>(column, opts, iter_mode)
                        // Not all tables has a prefix set, so we need to filter out the keys.
                        .take_while(move |item| {
                            if let Ok(item) = item {
                                T::starts_with(item, prefix.as_slice())
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
                self.iterator::<T>(column, self.read_options(), iter_mode)
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
                self.iterator::<T>(column, self.read_options(), iter_mode)
                    .take_while(move |item| {
                        if let Ok(item) = item {
                            T::starts_with(item, prefix.as_slice())
                        } else {
                            true
                        }
                    })
                    .into_boxed()
            }
        }
    }
}

pub(crate) struct KeyOnly;

impl ExtractItem for KeyOnly {
    type Item = Vec<u8>;

    fn extract_item<D>(
        raw_iterator: &DBRawIteratorWithThreadMode<D>,
    ) -> Option<Self::Item>
    where
        D: DBAccess,
    {
        raw_iterator.key().map(|key| key.to_vec())
    }

    fn size(item: &Self::Item) -> u64 {
        item.len() as u64
    }

    fn starts_with(item: &Self::Item, prefix: &[u8]) -> bool {
        item.starts_with(prefix)
    }
}

pub(crate) struct KeyAndValue;

impl ExtractItem for KeyAndValue {
    type Item = (Vec<u8>, Value);

    fn extract_item<D>(
        raw_iterator: &DBRawIteratorWithThreadMode<D>,
    ) -> Option<Self::Item>
    where
        D: DBAccess,
    {
        raw_iterator
            .item()
            .map(|(key, value)| (key.to_vec(), Arc::new(value.to_vec())))
    }

    fn size(item: &Self::Item) -> u64 {
        item.0.len().saturating_add(item.1.len()) as u64
    }

    fn starts_with(item: &Self::Item, prefix: &[u8]) -> bool {
        item.0.starts_with(prefix)
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
        self.metrics.read_meter.inc();
        let column_metrics = self.metrics.columns_read_statistic.get(&column.id());
        column_metrics.map(|metric| metric.inc());

        Ok(self
            .db
            .get_pinned_cf_opt(&self.cf(column), key, &self.read_options)
            .map_err(|e| DatabaseError::Other(e.into()))?
            .map(|value| value.len()))
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.metrics.read_meter.inc();
        let column_metrics = self.metrics.columns_read_statistic.get(&column.id());
        column_metrics.map(|metric| metric.inc());

        let value = self
            .db
            .get_cf_opt(&self.cf(column), key, &self.read_options)
            .map_err(|e| DatabaseError::Other(e.into()))?;

        if let Some(value) = &value {
            self.metrics.bytes_read.inc_by(value.len() as u64);
        }

        Ok(value.map(Arc::new))
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        mut buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        self.metrics.read_meter.inc();
        let column_metrics = self.metrics.columns_read_statistic.get(&column.id());
        column_metrics.map(|metric| metric.inc());

        let r = self
            .db
            .get_pinned_cf_opt(&self.cf(column), key, &self.read_options)
            .map_err(|e| DatabaseError::Other(e.into()))?
            .map(|value| {
                let read = value.len();
                std::io::Write::write_all(&mut buf, value.as_ref())
                    .map_err(|e| DatabaseError::Other(anyhow::anyhow!(e)))?;
                StorageResult::Ok(read)
            })
            .transpose()?;

        if let Some(r) = &r {
            self.metrics.bytes_read.inc_by(*r as u64);
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
        self._iter_store::<KeyAndValue>(column, prefix, start, direction)
    }

    fn iter_store_keys(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KeyItem> {
        self._iter_store::<KeyOnly>(column, prefix, start, direction)
    }
}

impl<Description> RocksDb<Description>
where
    Description: DatabaseDescription,
{
    pub fn commit_changes(&self, changes: &Changes) -> StorageResult<()> {
        let instant = std::time::Instant::now();
        let mut batch = WriteBatch::default();

        for (column, ops) in changes {
            let cf = self.cf_u32(*column);
            let column_metrics = self.metrics.columns_write_statistic.get(column);
            for (key, op) in ops {
                self.metrics.write_meter.inc();
                column_metrics.map(|metric| metric.inc());
                match op {
                    WriteOperation::Insert(value) => {
                        self.metrics.bytes_written.inc_by(value.len() as u64);
                        batch.put_cf(&cf, key, value.as_ref());
                    }
                    WriteOperation::Remove => {
                        batch.delete_cf(&cf, key);
                    }
                }
            }
        }

        self.db
            .write(batch)
            .map_err(|e| DatabaseError::Other(e.into()))?;
        // TODO: Use `u128` when `AtomicU128` is stable.
        self.metrics.database_commit_time.inc_by(
            u64::try_from(instant.elapsed().as_nanos())
                .expect("The commit shouldn't take longer than `u64`"),
        );

        Ok(())
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

#[cfg(feature = "test-helpers")]
pub mod test_helpers {
    use super::*;
    use fuel_core_storage::{
        kv_store::KeyValueMutate,
        transactional::ReadTransaction,
    };

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
            self.commit_changes(&changes)?;

            Ok(len)
        }

        fn delete(&mut self, key: &[u8], column: Self::Column) -> StorageResult<()> {
            let mut transaction = self.read_transaction();
            transaction.delete(key, column)?;
            let changes = transaction.into_changes();
            self.commit_changes(&changes)?;
            Ok(())
        }
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::database_description::on_chain::OnChain;
    use fuel_core_storage::{
        column::Column,
        kv_store::KeyValueMutate,
    };
    use std::collections::{
        BTreeMap,
        HashMap,
    };
    use tempfile::TempDir;

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

        db.commit_changes(&HashMap::from_iter(ops)).unwrap();
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
        db.commit_changes(&HashMap::from_iter(ops)).unwrap();

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
        let columns = enum_iterator::all::<<OnChain as DatabaseDescription>::Column>()
            .collect::<Vec<_>>();
        let result = RocksDb::<OnChain>::open(tmp_dir.path(), columns, None);

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

    #[test]
    fn snapshot_allows_get_entry_after_it_was_removed() {
        let (mut db, _tmp) = create_db();
        let value = Arc::new(vec![1, 2, 3]);

        // Given
        let key_1 = [1; 32];
        db.put(&key_1, Column::Metadata, value.clone()).unwrap();
        let snapshot = db.create_snapshot();

        // When
        db.delete(&key_1, Column::Metadata).unwrap();

        // Then
        let db_get = db.get(&key_1, Column::Metadata).unwrap();
        assert!(db_get.is_none());

        let snapshot_get = snapshot.get(&key_1, Column::Metadata).unwrap();
        assert_eq!(snapshot_get, Some(value));
    }

    #[test]
    fn snapshot_allows_correct_iteration_even_after_all_elements_where_removed() {
        let (mut db, _tmp) = create_db();
        let value = Arc::new(vec![1, 2, 3]);

        // Given
        let key_1 = [1; 32];
        let key_2 = [2; 32];
        let key_3 = [3; 32];
        db.put(&key_1, Column::Metadata, value.clone()).unwrap();
        db.put(&key_2, Column::Metadata, value.clone()).unwrap();
        db.put(&key_3, Column::Metadata, value.clone()).unwrap();
        let snapshot = db.create_snapshot();

        // When
        db.delete(&key_1, Column::Metadata).unwrap();
        db.delete(&key_2, Column::Metadata).unwrap();
        db.delete(&key_3, Column::Metadata).unwrap();

        // Then
        let db_iter = db
            .iter_store(Column::Metadata, None, None, IterDirection::Forward)
            .collect::<Vec<_>>();
        assert!(db_iter.is_empty());

        let snapshot_iter = snapshot
            .iter_store(Column::Metadata, None, None, IterDirection::Forward)
            .collect::<Vec<_>>();
        assert_eq!(
            snapshot_iter,
            vec![
                Ok((key_1.to_vec(), value.clone())),
                Ok((key_2.to_vec(), value.clone())),
                Ok((key_3.to_vec(), value))
            ]
        );
    }

    #[test]
    fn drop_snapshot_after_dropping_main_database_shouldn_panic() {
        let (db, _tmp) = create_db();

        // Given
        let snapshot = db.create_snapshot();

        // When
        drop(db);

        // Then
        drop(snapshot);
    }

    #[test]
    fn open__opens_subset_of_columns_after_opening_all_columns() {
        // Given
        let (first_open_with_all_columns, tmp_dir) = create_db();

        // When
        drop(first_open_with_all_columns);
        let part_of_columns =
            enum_iterator::all::<<OnChain as DatabaseDescription>::Column>()
                .skip(1)
                .collect::<Vec<_>>();
        let open_with_part_of_columns =
            RocksDb::<OnChain>::open(tmp_dir.path(), part_of_columns, None);

        // Then
        let _ = open_with_part_of_columns
            .expect("Should open the database with shorter number of columns");
    }
}
