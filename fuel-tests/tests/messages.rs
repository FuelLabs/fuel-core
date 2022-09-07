use fuel_core::{
    chain_config::{
        MessageConfig,
        StateConfig,
    },
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_interfaces::common::fuel_tx::TransactionBuilder;
use fuel_crypto::{
    fuel_types::Address,
    SecretKey,
};
use fuel_gql_client::client::{
    FuelClient,
    PageDirection,
    PaginationRequest,
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::ops::Deref;

#[tokio::test]
async fn can_submit_genesis_message() {
    let mut rng = StdRng::seed_from_u64(1234);

    let secret_key: SecretKey = rng.gen();
    let owner = secret_key.public_key().hash();

    let msg1 = MessageConfig {
        sender: rng.gen(),
        recipient: rng.gen(),
        owner: (*owner.deref()).into(),
        nonce: rng.gen(),
        amount: rng.gen(),
        data: vec![rng.gen()],
        da_height: 0,
    };
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .add_unsigned_message_input(
            secret_key,
            msg1.sender,
            msg1.recipient,
            msg1.nonce,
            msg1.amount,
            msg1.data.clone(),
        )
        .finalize();

    let mut node_config = Config::local_node();
    node_config.chain_conf.initial_state = Some(StateConfig {
        messages: Some(vec![msg1]),
        ..Default::default()
    });
    node_config.utxo_validation = true;

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    client.submit(&tx1).await.unwrap();
}

#[tokio::test]
async fn messages_returns_messages_for_all_owners() {
    // create some owners
    let owner_a = Address::new([1; 32]);
    let owner_b = Address::new([2; 32]);

    // create some messages for owner A
    let first_msg = MessageConfig {
        owner: owner_a,
        nonce: 1,
        ..Default::default()
    };
    let second_msg = MessageConfig {
        owner: owner_a,
        nonce: 2,
        ..Default::default()
    };

    // create a message for owner B
    let third_msg = MessageConfig {
        owner: owner_b,
        nonce: 3,
        ..Default::default()
    };

    // configure the messages
    let mut config = Config::local_node();
    config.chain_conf.initial_state = Some(StateConfig {
        messages: Some(vec![first_msg, second_msg, third_msg]),
        ..Default::default()
    });

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // get the messages
    let request = PaginationRequest {
        cursor: None,
        results: 5,
        direction: PageDirection::Forward,
    };
    let result = client.messages(None, request).await.unwrap();

    // verify that there are 3 messages stored in total
    assert_eq!(result.results.len(), 3);
}

#[tokio::test]
async fn messages_by_owner_returns_messages_for_the_given_owner() {
    // create some owners
    let owner_a = Address::new([1; 32]);
    let owner_b = Address::new([2; 32]);

    // create some messages for owner A
    let first_msg = MessageConfig {
        owner: owner_a,
        nonce: 1,
        ..Default::default()
    };
    let second_msg = MessageConfig {
        owner: owner_a,
        nonce: 2,
        ..Default::default()
    };

    // create a message for owner B
    let third_msg = MessageConfig {
        owner: owner_b,
        nonce: 3,
        ..Default::default()
    };

    // configure the messages
    let mut config = Config::local_node();
    config.chain_conf.initial_state = Some(StateConfig {
        messages: Some(vec![first_msg, second_msg, third_msg]),
        ..Default::default()
    });

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let request = PaginationRequest {
        cursor: None,
        results: 5,
        direction: PageDirection::Forward,
    };

    // get the messages from Owner A
    let result = client
        .messages(Some(&owner_a.to_string()), request.clone())
        .await
        .unwrap();

    // verify that Owner A has 2 messages
    assert_eq!(result.results.len(), 2);

    // verify messages owner matches
    for message in result.results {
        assert_eq!(message.owner.0 .0, owner_a)
    }

    // get the messages from Owner B
    let result = client
        .messages(Some(&owner_b.to_string()), request.clone())
        .await
        .unwrap();

    // verify that Owner B has 1 message
    assert_eq!(result.results.len(), 1);

    assert_eq!(result.results[0].owner.0 .0, owner_b);
}

#[tokio::test]
async fn messages_empty_results_for_owner_with_no_messages() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let owner = Address::new([1; 32]);
    let request = PaginationRequest {
        cursor: None,
        results: 5,
        direction: PageDirection::Forward,
    };

    let result = client
        .messages(Some(&owner.to_string()), request)
        .await
        .unwrap();

    assert_eq!(result.results.len(), 0);
}
