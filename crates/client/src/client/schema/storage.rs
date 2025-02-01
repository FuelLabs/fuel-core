use crate::client::schema::{
    schema,
    Bytes32,
    ContractId,
    HexString,
    U32,
};
use fuel_core_types::fuel_tx;

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Subscription",
    variables = "AllStorageSlotsArgs"
)]
pub struct AllStorageSlots {
    #[arguments(contractId: $contract_id)]
    pub all_storage_slots: StorageSlot,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct AllStorageSlotsArgs {
    /// The ID of the contract.
    pub contract_id: ContractId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "ContractStorageValuesArgs"
)]
pub struct ContractStorageValues {
    #[arguments(contractId: $contract_id, blockHeight: $block_height, storageSlots: $storage_slots)]
    pub contract_storage_values: Vec<StorageSlot>,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct ContractStorageValuesArgs {
    pub contract_id: ContractId,
    pub block_height: Option<U32>,
    pub storage_slots: Vec<Bytes32>,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct StorageSlot {
    pub key: Bytes32,
    pub value: HexString,
}

impl From<StorageSlot> for (fuel_tx::Bytes32, Vec<u8>) {
    fn from(value: StorageSlot) -> Self {
        (value.key.into(), value.value.into())
    }
}
