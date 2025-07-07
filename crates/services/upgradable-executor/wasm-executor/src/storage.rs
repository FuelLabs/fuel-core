use super::ext;
use fuel_core_storage::{
    Direction,
    NextEntry,
    Result as StorageResult,
    column::Column,
    kv_store::{
        Key,
        KeyValueInspect,
        Value,
    },
};

pub struct WasmStorage;

impl KeyValueInspect for WasmStorage {
    type Column = Column;

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        ext::size_of_value(key, column.as_u32()).map_err(Into::into)
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        let size = ext::size_of_value(key, column.as_u32())?;

        if let Some(size) = size {
            let mut value = vec![0u8; size];
            ext::get(key, column.as_u32(), &mut value)?;
            Ok(Some(value.into()))
        } else {
            Ok(None)
        }
    }

    fn get_next(
        &self,
        _: &[u8],
        _: Self::Column,
        _: Direction,
        _: usize,
    ) -> StorageResult<NextEntry<Key, Value>> {
        // TODO: Implement this method to retrieve the next entry in the storage.
        //  It requires breaking to WASM interface and hard-fork of the network.
        todo!()
    }
}
