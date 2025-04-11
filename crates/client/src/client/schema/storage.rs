use crate::client::schema::{
    AssetId,
    Bytes32,
    ContractId,
    HexString,
    U32,
    contract::ContractBalance,
    schema,
};
use fuel_core_types::fuel_tx;

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Subscription",
    variables = "ContractStorageSlotsArgs"
)]
pub struct ContractStorageSlots {
    #[arguments(contractId: $contract_id)]
    pub contract_storage_slots: StorageSlot,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct ContractStorageSlotsArgs {
    /// The ID of the contract.
    pub contract_id: ContractId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "ContractSlotValuesArgs"
)]
pub struct ContractSlotValues {
    #[arguments(contractId: $contract_id, blockHeight: $block_height, storageSlots: $storage_slots)]
    pub contract_slot_values: Vec<StorageSlot>,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct ContractSlotValuesArgs {
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

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Subscription",
    variables = "ContractStorageBalancesArgs"
)]
pub struct ContractStorageBalances {
    #[arguments(contractId: $contract_id)]
    pub contract_storage_balances: ContractBalance,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct ContractStorageBalancesArgs {
    /// The ID of the contract.
    pub contract_id: ContractId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "ContractBalanceValuesArgs"
)]
pub struct ContractBalanceValues {
    #[arguments(contractId: $contract_id, blockHeight: $block_height, assets: $assets)]
    pub contract_balance_values: Vec<ContractBalance>,
}

#[derive(cynic::QueryVariables, Debug)]
pub struct ContractBalanceValuesArgs {
    pub contract_id: ContractId,
    pub block_height: Option<U32>,
    pub assets: Vec<AssetId>,
}
