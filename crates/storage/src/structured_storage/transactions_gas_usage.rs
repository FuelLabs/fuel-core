//! The module contains implementations and tests for the
//! `TransactionsGasUsage` table.

use crate::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::TransactionsGasUsage,
};

impl TableWithBlueprint for TransactionsGasUsage {
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::TransactionsGasUsage
    }
}

#[cfg(test)]
crate::basic_storage_tests!(
    TransactionsGasUsage,
    <TransactionsGasUsage as crate::Mappable>::Key::default(),
    alloc::vec![10u64, 20u64, 30u64]
);
