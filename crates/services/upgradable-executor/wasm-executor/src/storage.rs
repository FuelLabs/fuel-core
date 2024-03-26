use super::ext;
use fuel_core_storage::{
    column::Column,
    kv_store::{
        KeyValueInspect,
        Value,
    },
    Result as StorageResult,
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
}
