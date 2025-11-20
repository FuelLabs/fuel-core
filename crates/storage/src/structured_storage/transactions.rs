//! The module contains implementations and tests for the `Transactions` table.

use crate::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::{
        ProcessedTransactions,
        Transactions,
    },
};

impl TableWithBlueprint for Transactions {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = Column;

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

impl TableWithBlueprint for ProcessedTransactions {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = Column;

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
