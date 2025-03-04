use fuel_core::service::FuelService;
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_asm::op,
    fuel_tx::TransactionBuilder,
};
use test_helpers::{
    assemble_tx::AssembleAndRunTx,
    config_with_fee,
    default_signing_wallet,
};

#[tokio::test]
async fn run_transaction() {
    let config = config_with_fee();
    let service = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(service.bound_address);

    // Given
    let tx = TransactionBuilder::script(vec![op::ret(1)].into_iter().collect(), vec![])
        .witness_limit(10000)
        .finalize_as_transaction();

    // When
    let result = client
        .assemble_transaction(&tx, default_signing_wallet(), vec![])
        .await;

    // Then
    let _ = result.expect("Failed to assemble transaction");
}
