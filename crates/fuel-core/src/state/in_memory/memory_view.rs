use crate::database::database_description::{
    on_chain::OnChain,
    DatabaseDescription,
};
use fuel_core_storage::{
    iter::{
        iterator,
        keys_iterator,
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyItem,
        KeyValueInspect,
        StorageColumn,
        Value,
    },
    transactional::ReferenceBytesKey,
    Result as StorageResult,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct MemoryView<Description = OnChain>
where
    Description: DatabaseDescription,
{
    pub(crate) inner: Vec<BTreeMap<ReferenceBytesKey, Value>>,
    pub(crate) _marker: core::marker::PhantomData<Description>,
}

impl<Description> MemoryView<Description>
where
    Description: DatabaseDescription,
{
    pub fn iter_all<'a>(
        &'a self,
        column: Description::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> impl Iterator<Item = KVItem> + 'a {
        let btree = &self.inner[column.as_usize()];

        iterator(btree, prefix, start, direction)
            .map(|(key, value)| (key.clone().into(), value.clone()))
            .map(Ok)
    }

    pub fn iter_all_keys(
        &self,
        column: Description::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> impl Iterator<Item = KeyItem> + '_ {
        let btree = &self.inner[column.as_usize()];

        keys_iterator(btree, prefix, start, direction)
            .map(|key| key.clone().into())
            .map(Ok)
    }
}

impl<Description> KeyValueInspect for MemoryView<Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        Ok(self.inner[column.as_usize()].get(key).cloned())
    }
}

impl<Description> IterableStore for MemoryView<Description>
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
        self.iter_all(column, prefix, start, direction).into_boxed()
    }

    fn iter_store_keys(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KeyItem> {
        self.iter_all_keys(column, prefix, start, direction)
            .into_boxed()
    }
}
