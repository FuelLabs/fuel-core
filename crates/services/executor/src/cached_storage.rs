use fuel_core_storage::{
    kv_store::{
        KeyValueInspect,
        StorageColumn,
        Value,
    },
    transactional::ReferenceBytesKey,
    Result as StorageResult,
};
use std::{
    cell::RefCell,
    collections::{
        BTreeMap,
        HashMap,
    },
};

pub struct CachedStorage<S> {
    cache: RefCell<HashMap<u32, BTreeMap<ReferenceBytesKey, Option<Value>>>>,
    storage: S,
}

impl<Column, S> CachedStorage<S>
where
    Column: StorageColumn,
    S: KeyValueInspect<Column = Column>,
{
    pub fn new(storage: S) -> Self {
        Self {
            cache: RefCell::new(HashMap::new()),
            storage,
        }
    }

    fn get_from_cache(&self, key: &[u8], column: Column) -> Option<Option<Value>> {
        self.cache
            .borrow()
            .get(&column.id())
            .and_then(|btree| btree.get(key))
            .cloned()
    }

    fn get_from_storage(
        &self,
        key: &[u8],
        column: Column,
    ) -> StorageResult<Option<Value>> {
        let result = self.storage.get(key, column)?;
        let id = column.id();

        self.cache
            .borrow_mut()
            .entry(id)
            .or_default()
            .insert(key.to_vec().into(), result.clone());

        Ok(result)
    }
}

impl<Column, S> KeyValueInspect for CachedStorage<S>
where
    Column: StorageColumn,
    S: KeyValueInspect<Column = Column>,
{
    type Column = Column;

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        if let Some(cache) = self.get_from_cache(key, column) {
            Ok(cache.clone())
        } else {
            self.get_from_storage(key, column)
        }
    }
}
