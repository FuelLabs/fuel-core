use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_gql_client::{
    client::FuelClient,
    fuel_tx::Transaction,
};

#[tokio::test]
async fn chain_info() {
    let node_config = Config::local_node();
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    client
        .submit_and_await_commit(&Transaction::default())
        .await
        .unwrap();

    let chain_info = client.chain_info().await.unwrap();

    assert_eq!(node_config.chain_conf.chain_name, chain_info.name);
    assert_eq!(
        node_config.chain_conf.transaction_parameters,
        chain_info.consensus_parameters.into()
    );
}
