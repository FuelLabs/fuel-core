use crate::{
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    column::Column,
    structure::plain::Plain,
    structured_storage::TableWithStructure,
    tables::Receipts,
};

impl TableWithStructure for Receipts {
    type Structure = Plain<Raw, Postcard>;

    fn column() -> Column {
        Column::Receipts
    }
}

crate::basic_storage_tests!(
    Receipts,
    <Receipts as crate::Mappable>::Key::from([1u8; 32]),
    vec![fuel_core_types::fuel_tx::Receipt::ret(
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default()
    )]
);
