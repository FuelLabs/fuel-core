//! The module contains implementations and tests for the `Transactions` table.

use crate::{
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    column::Column,
    structure::plain::Plain,
    structured_storage::TableWithStructure,
    tables::{
        ProcessedTransactions,
        Transactions,
    },
};

impl TableWithStructure for Transactions {
    type Structure = Plain<Raw, Postcard>;

    fn column() -> Column {
        Column::Transactions
    }
}

#[cfg(test)]
crate::basic_storage_tests!(
    Transactions,
    <Transactions as crate::Mappable>::Key::from([1u8; 32]),
    <Transactions as crate::Mappable>::Value::default()
);

impl TableWithStructure for ProcessedTransactions {
    type Structure = Plain<Raw, Postcard>;

    fn column() -> Column {
        Column::ProcessedTransactions
    }
}

#[cfg(test)]
crate::basic_storage_tests!(
    ProcessedTransactions,
    <ProcessedTransactions as crate::Mappable>::Key::from([1u8; 32]),
    <ProcessedTransactions as crate::Mappable>::Value::default()
);
