//! The module contains implementations and tests for the contracts tables.

use fuel_vm_private::storage::BlobData;

use crate::{
    blueprint::plain::Plain,
    codec::raw::Raw,
    column::Column,
    structured_storage::TableWithBlueprint,
};

impl TableWithBlueprint for BlobData {
    type Blueprint = Plain<Raw, Raw>;
    type Column = Column;

    fn column() -> Self::Column {
        Column::Blobs
    }
}

#[cfg(test)]
mod test {
    use super::*;

    crate::basic_storage_tests!(
        BlobData,
        <BlobData as crate::Mappable>::Key::default(),
        vec![32u8],
        <BlobData as crate::Mappable>::OwnedValue::from(vec![32u8])
    );
}
