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
    entities::relayer::transaction::RelayedTransactionStatus,
    fuel_tx::Bytes32,
};

/// Tracks the status of transactions from the L1. These are tracked separately from tx-pool
/// transactions because they might fail as part of the relay process, not just due
/// to execution.
pub struct RelayedTransactionStatuses;

impl Mappable for RelayedTransactionStatuses {
    type Key = Bytes32;
    type OwnedKey = Self::Key;
    type Value = RelayedTransactionStatus;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for RelayedTransactionStatuses {
    type Blueprint = Plain<Raw, Postcard>;

    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::RelayedTransactionStatus
    }
}

impl AsTable<RelayedTransactionStatuses> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<RelayedTransactionStatuses>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<RelayedTransactionStatuses> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<RelayedTransactionStatuses>>) {
        // Do not include these for now
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::fuel_tx::Bytes32;

    fuel_core_storage::basic_storage_tests!(
        RelayedTransactionStatuses,
        <RelayedTransactionStatuses as Mappable>::Key::from(Bytes32::default()),
        RelayedTransactionStatus::Failed {
            block_height: 0.into(),
            failure: "Some reason".to_string(),
        }
    );
}
