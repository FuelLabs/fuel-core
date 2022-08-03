use fuel_core::{config::Config, service::FuelService};
use fuel_core_interfaces::model::DaMessage;
use fuel_gql_client::client::FuelClient;

#[tokio::test]
async fn chain_info() {
    let node_config = Config::local_node();
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let chain_info = client.chain_info().await.unwrap();

    assert_eq!(node_config.chain_conf.chain_name, chain_info.name);
    assert_eq!(
        node_config.chain_conf.transaction_parameters,
        chain_info.consensus_parameters.into()
    );
}

#[tokio::test]
async fn test_da_messages() {
    let mut node_config = Config::local_node();

    let msg1 = DaMessage {
        fuel_block_spend: Some(1u64.into()),
        ..Default::default()
    };

    let msg2 = DaMessage {
        fuel_block_spend: Some(6u64.into()),
        ..Default::default()
    };
    
    let msg3 = DaMessage {
        fuel_block_spend: Some(10u64.into()),
        ..Default::default()
    };


    let test_msgs = vec![msg1, msg2, msg3];

    node_config.chain_conf.da_messages = Some(test_msgs); 

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);



}
