use fuel_core_chain_config::{
    AddTable,
    AsTable,
    StateConfig,
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
    entities::contract::ContractsInfoType,
    fuel_tx::{
        ContractId,
        Salt,
    },
};

/// Contract info
pub struct ContractsInfo;

impl Mappable for ContractsInfo {
    type Key = Self::OwnedKey;
    type OwnedKey = ContractId;
    type Value = Self::OwnedValue;
    type OwnedValue = ContractsInfoType;
}

impl TableWithBlueprint for ContractsInfo {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::ContractsInfo
    }
}

impl AsTable<ContractsInfo> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<ContractsInfo>> {
        self.contracts
            .iter()
            .map(|contracts_config| TableEntry {
                key: contracts_config.contract_id,
                value: ContractsInfoType::V1(Salt::zeroed().into()),
            })
            .collect()
    }
}

impl AddTable<ContractsInfo> for StateConfig {
    fn add(&mut self, _entries: Vec<TableEntry<ContractsInfo>>) {
        // Do not include these
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use fuel_core_types::fuel_tx::Salt;

    fuel_core_storage::basic_storage_tests!(
        ContractsInfo,
        <ContractsInfo as Mappable>::Key::from([1u8; 32]),
        ContractsInfoType::V1(Salt::new([2u8; 32]).into())
    );
}
