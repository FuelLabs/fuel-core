use fuel_core::{
    database::Database,
    service::{Config, FuelService},
};
use fuel_gql_client::client::FuelClient;
use fuel_storage::Storage;
use fuel_vm::prelude::Contract as FuelVmContract;

#[tokio::test]
async fn contract() {
    // setup test data in the node
    let contract = FuelVmContract::default();
    let id = fuel_types::ContractId::new(Default::default());

    let mut db = Database::default();
    Storage::<fuel_types::ContractId, FuelVmContract>::insert(&mut db, &id, &contract).unwrap();
    // setup server & client
    let srv = FuelService::from_database(db, Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    let contract = client
        .contract(format!("{:#x}", id).as_str())
        .await
        .unwrap();
    assert!(contract.is_some());
}
