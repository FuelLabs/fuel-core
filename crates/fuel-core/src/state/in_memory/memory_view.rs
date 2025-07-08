use crate::database::database_description::{
    DatabaseDescription,
    on_chain::OnChain,
};
use fuel_core_storage::{
    Direction,
    NextEntry,
    Result as StorageResult,
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
        IterableStore,
        iterator,
        keys_iterator,
    },
    kv_store::{
        KVItem,
        Key,
        KeyItem,
        KeyValueInspect,
        StorageColumn,
        Value,
    },
    transactional::ReferenceBytesKey,
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
    ) -> impl Iterator<Item = KVItem> + 'a + use<'a, Description> {
        let btree = &self.inner[column.as_usize()];

        iterator(btree, prefix, start, direction)
            .map(|(key, value)| (key.clone(), value.clone()))
            .map(Ok)
    }

    pub fn iter_all_keys(
        &self,
        column: Description::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> impl Iterator<Item = KeyItem> + '_ + use<'_, Description> {
        let btree = &self.inner[column.as_usize()];

        keys_iterator(btree, prefix, start, direction)
            .cloned()
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

    fn get_next(
        &self,
        start_key: &[u8],
        column: Self::Column,
        direction: Direction,
        max_iterations: usize,
    ) -> StorageResult<NextEntry<Key, Value>> {
        let btree = &self.inner[column.as_usize()];

        let next = direction.next_from_map(start_key, btree, max_iterations);

        Ok(next)
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
