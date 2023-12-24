use crate::{
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    column::Column,
    structure::plain::Plain,
    structured_storage::TableWithStructure,
    tables::SealedBlockConsensus,
};

impl TableWithStructure for SealedBlockConsensus {
    type Structure = Plain<Raw, Postcard>;

    fn column() -> Column {
        Column::FuelBlockConsensus
    }
}

crate::basic_storage_tests!(
    SealedBlockConsensus,
    <SealedBlockConsensus as crate::Mappable>::Key::from([1u8; 32]),
    <SealedBlockConsensus as crate::Mappable>::Value::default()
);
