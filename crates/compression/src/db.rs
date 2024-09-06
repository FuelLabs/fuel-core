use std::path::Path;

use anyhow::bail;
use fuel_core_types::{
    fuel_compression::RegistryKey,
    fuel_types::BlockHeight,
};

use crate::tables::RegistryKeyspace;

/// Database that holds data needed by the block compression only
pub struct RocksDb {
    pub(crate) db: rocksdb::DB,
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
                    // Meta table holding misc data
                    ColumnFamilyDescriptor::new("meta", Options::default()),
                    // Temporal registry key:value pairs, with key as
                    // null-separated (table, key) pair
                    ColumnFamilyDescriptor::new("temporal", Options::default()),
                    // Reverse index into temporal registry values, with key as
                    // null-separated (table, indexed_value) pair
                    ColumnFamilyDescriptor::new("temporal_index", Options::default()),
                ],
            )?,
        })
    }
}

impl RocksDb {
    pub fn read_registry<T>(
        &self,
        keyspace: RegistryKeyspace,
        key: RegistryKey,
    ) -> anyhow::Result<T>
    where
        T: serde::de::DeserializeOwned + Default,
    {
        if key == RegistryKey::DEFAULT_VALUE {
            return Ok(T::default());
        }

        let db_key: Vec<u8> =
            keyspace.name().bytes().chain(core::iter::once(0)).collect();
        let db_key = postcard::to_extend(&key, db_key).expect("Never fails");

        let cf = self.db.cf_handle("temporal").unwrap();
        let Some(bytes) = self.db.get_cf(&cf, &db_key)? else {
            bail!("Key {keyspace:?}:{key:?} not found");
        };
        Ok(postcard::from_bytes(&bytes)?)
    }

    pub fn registry_index_lookup<V: serde::Serialize>(
        &self,
        keyspace: RegistryKeyspace,
        value: V,
    ) -> anyhow::Result<Option<RegistryKey>> {
        let db_key: Vec<u8> =
            keyspace.name().bytes().chain(core::iter::once(0)).collect();
        let db_key = postcard::to_extend(&value, db_key).expect("Never fails");

        let cf_index = self.db.cf_handle("temporal_index").unwrap();
        let Some(k) = self.db.get_cf(&cf_index, db_key)? else {
            return Ok(None);
        };
        Ok(Some(postcard::from_bytes(&k)?))
    }
}

impl RocksDb {
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
