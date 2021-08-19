use crate::state::ColumnId;

pub mod memory_store;
pub mod transaction;

pub(crate) fn column_key(key: &[u8], column: ColumnId) -> Vec<u8> {
    let mut ck = column.to_be_bytes().to_vec();
    ck.extend_from_slice(key);
    ck
}

pub(crate) fn is_column(column_key: &[u8], column: ColumnId) -> bool {
    let column_bytes = column.to_be_bytes();
    column_key[..column_bytes.len()] == column_bytes
}
