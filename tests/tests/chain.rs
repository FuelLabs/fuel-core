use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::FuelClient;

#[tokio::test]
async fn chain_info() {
    let node_config = Config::local_node();
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let chain_info = client.chain_info().await.unwrap();

    assert_eq!(0, chain_info.da_height);
    assert_eq!(node_config.chain_conf.chain_name, chain_info.name);
    assert_eq!(
        node_config.chain_conf.consensus_parameters,
        chain_info.consensus_parameters.clone()
    );

    assert_eq!(
        node_config.chain_conf.consensus_parameters.gas_costs,
        chain_info.consensus_parameters.gas_costs
    );
}
