use fuel_core_chain_config::{
    AddTable,
    AsTable,
    StateConfig,
    StateConfigBuilder,
    TableEntry,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::{
    fuel_tx::{
        Bytes32,
        // Receipt,
    },
    // fuel_types::BlockHeight,
    // fuel_vm::ProgramState,
    services::txpool::TransactionStatus,
    // tai64::Tai64,
};

pub struct LayerOneTransactionStatuses;

// #[derive(Clone, Debug)]
// pub struct LayerOneTransactionIdentifier(Bytes32);
//
// impl From<Bytes32> for LayerOneTransactionIdentifier {
//     fn from(bytes: Bytes32) -> Self {
//         Self(bytes)
//     }
// }
//
// impl From<LayerOneTransactionIdentifier> for Bytes32 {
//     fn from(identifier: LayerOneTransactionIdentifier) -> Self {
//         identifier.0
//     }
// }

// #[derive(Clone, Debug, PartialEq, Eq)]
// pub enum LayerOneTransactionStatus {
//     /// Transaction was included in a block, but the execution was reverted
//     Failed {
//         /// Included in this block
//         block_height: BlockHeight,
//         /// Time when the block was generated
//         time: Tai64,
//         /// Result of executing the transaction for scripts
//         result: Option<ProgramState>,
//         /// The receipts generated during execution of the transaction.
//         receipts: Vec<Receipt>,
//     },
// }

impl Mappable for LayerOneTransactionStatuses {
    type Key = Bytes32;
    type OwnedKey = Self::Key;
    type Value = TransactionStatus;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for LayerOneTransactionStatuses {
    type Blueprint = Plain<Raw, Postcard>;

    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::LayerOneTransactionStatus
    }
}

impl AsTable<LayerOneTransactionStatuses> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<LayerOneTransactionStatuses>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<LayerOneTransactionStatuses> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<LayerOneTransactionStatuses>>) {
        // Do not include these for now
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::{
        services::txpool::TransactionStatus,
        tai64::Tai64,
    };

    fuel_core_storage::basic_storage_tests!(
        LayerOneTransactionStatuses,
        <LayerOneTransactionStatuses as Mappable>::Key::from(Bytes32::default()),
        // LayerOneTransactionStatus::Failed {
        //     block_height: 0.into(),
        //     time: Tai64::UNIX_EPOCH,
        //     result: None,
        //     receipts: Vec::new(),
        // }
        TransactionStatus::Failed {
            block_height: 0.into(),
            time: Tai64::UNIX_EPOCH,
            result: None,
            receipts: Vec::new(),
        }
    );
}
