use fuel_core::{config::Config, service::FuelService};
use fuel_core_interfaces::{common::fuel_tx::TransactionBuilder, model::DaMessage};
use fuel_gql_client::client::FuelClient;
use fuel_gql_client::fuel_tx::Input;

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

fn create_message_predicate_from_message(message: &DaMessage) -> Input {
    Input::message_predicate(
        message.id(),
        message.sender,
        message.recipient,
        message.amount,
        message.nonce,
        message.owner,
        message.data.clone(),
        Default::default(),
        Default::default(),
    )
}

#[tokio::test]
async fn test_da_messages() {
    let mut node_config = Config::local_node();

    let msg1 = DaMessage {
        fuel_block_spend: Some(1u64.into()),
        ..Default::default()
    };
/*
    let msg2 = DaMessage {
        fuel_block_spend: Some(6u64.into()),
        ..Default::default()
    };

    let msg3 = DaMessage {
        fuel_block_spend: Some(10u64.into()),
        ..Default::default()
    };
*/
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .add_input(create_message_predicate_from_message(&msg1))
        .finalize();
/*
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .add_input(create_message_predicate_from_message(&msg2))
        .finalize();

    let tx3 = TransactionBuilder::script(vec![], vec![])
        .add_input(create_message_predicate_from_message(&msg3))
        .finalize();
*/
    let test_msgs = vec![msg1];

    node_config.chain_conf.da_messages = Some(test_msgs);

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    client.submit(&tx1).await.unwrap();
    // client.submit(&tx2).await.unwrap();
    // client.submit(&tx3).await.unwrap();
}
