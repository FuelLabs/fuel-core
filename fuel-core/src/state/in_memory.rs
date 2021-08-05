use crate::state::ColumnId;

pub mod memory_store;
pub mod transaction;

pub(crate) fn column_key(key: &[u8], column: ColumnId) -> Vec<u8> {
    let mut ck = key.as_ref().iter().copied().collect::<Vec<u8>>();
    ck.extend_from_slice(&column.to_be_bytes());
    ck
}
