use std::path::Path;

use fuel_core_types::{
    fuel_compression::{
        Key,
        KeyPerTable,
        RawKey,
        Table,
        TableName,
    },
    fuel_types::BlockHeight,
};
use rocksdb::WriteBatchWithTransaction;

pub struct RocksDb {
    db: rocksdb::DB,
}

impl RocksDb {
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        use rocksdb::{
            ColumnFamilyDescriptor,
            Options,
            DB,
        };

        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        Ok(Self {
            db: DB::open_cf_descriptors(
                &db_opts,
                path,
                vec![
                    // Meta table holding misc
                    ColumnFamilyDescriptor::new("meta", Options::default()),
                    // Next temporal registry key for each table
                    ColumnFamilyDescriptor::new("next_keys", Options::default()),
                    // Temporal registry key:value pairs, with key as
                    // null-separated (table, key) pair
                    ColumnFamilyDescriptor::new("temporal", Options::default()),
                    // Reverse index into temporal registry values, with key as
                    // null-separated (table, indexed_value) pair
                    ColumnFamilyDescriptor::new("index", Options::default()),
                ],
            )?,
        })
    }
}

impl RocksDb {
    fn next_key_raw(&self, table: TableName) -> anyhow::Result<RawKey> {
        let cf_next_keys = self.db.cf_handle("next_keys").unwrap();
        let Some(bytes) = self.db.get_cf(&cf_next_keys, table)? else {
            return Ok(RawKey::ZERO);
        };
        Ok(postcard::from_bytes(&bytes).expect("Invalid key"))
    }

    fn next_key<T: Table>(&self) -> anyhow::Result<Key<T>> {
        Ok(Key::from_raw(self.next_key_raw(T::NAME)?))
    }

    pub fn start_keys(&self) -> anyhow::Result<KeyPerTable> {
        // TODO: allow KeyPerTable::from_fn to pass through errors
        Ok(KeyPerTable::from_fn(|table_name| {
            self.next_key_raw(table_name).expect("Db error")
        }))
    }

    pub fn read<T: Table>(&self, key: Key<T>) -> anyhow::Result<T::Type> {
        if key == Key::DEFAULT_VALUE {
            return Ok(T::Type::default());
        }

        let bare_key = postcard::to_stdvec(&key).expect("Never fails");

        let mut table_suffix: Vec<u8> = T::NAME.bytes().collect();
        table_suffix.push(0);

        // Write new value
        let k: Vec<u8> = table_suffix
            .iter()
            .chain(bare_key.iter())
            .copied()
            .collect();

        let cf = self.db.cf_handle("temporal").unwrap();
        let Some(bytes) = self.db.get_cf(&cf, &k)? else {
            return Ok(T::Type::default());
        };
        Ok(postcard::from_bytes(&bytes).expect("Invalid value"))
    }

    pub fn batch_write<T: Table>(
        &mut self,
        start_key: Key<T>,
        values: Vec<T::Type>,
    ) -> anyhow::Result<()> {
        let mut key = start_key;

        let mut batch = WriteBatchWithTransaction::<false>::default();

        let cf_registry = self.db.cf_handle("temporal").unwrap();
        let cf_index = self.db.cf_handle("index").unwrap();

        let empty = values.is_empty();
        for value in values.into_iter() {
            let bare_key = postcard::to_stdvec(&key).expect("Never fails");
            let v = postcard::to_stdvec(&value).expect("Never fails");

            let mut table_suffix: Vec<u8> = T::NAME.bytes().collect();
            table_suffix.push(0);

            // Write new value
            let k: Vec<u8> = table_suffix
                .iter()
                .chain(bare_key.iter())
                .copied()
                .collect();
            batch.put_cf(&cf_registry, k.clone(), v.clone());

            // Remove the overwritten value from index, if any
            if let Some(old) = self.db.get_cf(&cf_registry, k)? {
                let iv: Vec<u8> = table_suffix.clone().into_iter().chain(old).collect();
                batch.delete_cf(&cf_index, iv);
            }

            // Add it to the index
            let iv: Vec<u8> = table_suffix.into_iter().chain(v).collect();
            batch.put_cf(&cf_index, iv, bare_key);

            key = key.next();
        }
        self.db.write(batch)?;

        if !empty {
            let key = postcard::to_stdvec(&key).expect("Never fails");
            let cf_next_keys = self.db.cf_handle("next_keys").unwrap();
            self.db.put_cf(&cf_next_keys, T::NAME, key)?;
        }

        Ok(())
    }

    pub fn index_lookup<T: Table>(
        &self,
        value: &T::Type,
    ) -> anyhow::Result<Option<Key<T>>> {
        let cf_index = self.db.cf_handle("index").unwrap();
        let val = postcard::to_stdvec(&value).expect("Never fails");
        let mut key: Vec<u8> = T::NAME.bytes().collect();
        key.push(0);
        key.extend(val);
        let Some(k) = self.db.get_cf(&cf_index, key)? else {
            return Ok(None);
        };
        Ok(Some(postcard::from_bytes(&k).expect("Never fails")))
    }

    pub fn next_block_height(&self) -> anyhow::Result<BlockHeight> {
        let cf_meta = self.db.cf_handle("meta").unwrap();
        let Some(bytes) = self.db.get_cf(&cf_meta, b"current_block")? else {
            return Ok(BlockHeight::default());
        };
        debug_assert!(bytes.len() == 4);
        let mut buffer = [0u8; 4];
        buffer.copy_from_slice(&bytes[..]);
        Ok(BlockHeight::from(buffer))
    }

    pub fn increment_block_height(&self) -> anyhow::Result<()> {
        // TODO: potential TOCTOU bug here
        let cf_meta = self.db.cf_handle("meta").unwrap();
        let old_bh = match self.db.get_cf(&cf_meta, b"current_block")? {
            Some(bytes) => {
                debug_assert!(bytes.len() == 4);
                let mut buffer = [0u8; 4];
                buffer.copy_from_slice(&bytes[..]);
                BlockHeight::from(buffer)
            }
            None => BlockHeight::default(),
        };
        let new_bh = old_bh
            .succ()
            .ok_or_else(|| anyhow::anyhow!("Block height overflow"))?;
        self.db
            .put_cf(&cf_meta, b"current_block", new_bh.to_bytes())?;
        Ok(())
    }
}
