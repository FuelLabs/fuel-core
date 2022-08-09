use fuel_core::{
    config::{
        chain_config::{DaMessageConfig, StateConfig},
        Config,
    },
    database::Database,
    service::FuelService,
};
use fuel_core_interfaces::common::fuel_storage::Storage;
use fuel_core_interfaces::common::fuel_tx::TransactionBuilder;
use fuel_core_interfaces::model::Message;
use fuel_crypto::fuel_types::{Address, MessageId};
use fuel_crypto::SecretKey;
use fuel_gql_client::client::{FuelClient, PageDirection, PaginationRequest};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::ops::Deref;

#[tokio::test]
async fn can_submit_genesis_message() {
    let mut rng = StdRng::seed_from_u64(1234);

    let secret_key: SecretKey = rng.gen();
    let owner = secret_key.public_key().hash();

    let msg1 = DaMessageConfig {
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
    // setup server & client
    let mut db = Database::default();
    let srv = FuelService::from_database(db.clone(), Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    // create some owners
    let owner_a = Address::new([1; 32]);
    let owner_b = Address::new([2; 32]);

    // create some messages for owner A
    let first_msg = Message {
        owner: owner_a,
        ..Default::default()
    };
    let second_msg = Message {
        owner: owner_a,
        ..Default::default()
    };

    // create a message for owner B
    let third_msg = Message {
        owner: owner_b,
        ..Default::default()
    };

    // store the messages
    let first_id = MessageId::new([1; 32]);
    let _ = Storage::<MessageId, Message>::insert(&mut db, &first_id, &first_msg).unwrap();

    let second_id = MessageId::new([2; 32]);
    let _ = Storage::<MessageId, Message>::insert(&mut db, &second_id, &second_msg).unwrap();

    let third_id = MessageId::new([3; 32]);
    let _ = Storage::<MessageId, Message>::insert(&mut db, &third_id, &third_msg).unwrap();

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
    // setup server & client
    let mut db = Database::default();
    let srv = FuelService::from_database(db.clone(), Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    // create some owners
    let owner_a = Address::new([1; 32]);
    let owner_b = Address::new([2; 32]);

    // create some messages for owner A
    let first_msg = Message {
        owner: owner_a,
        ..Default::default()
    };
    let second_msg = Message {
        owner: owner_a,
        ..Default::default()
    };

    // create a message for owner B
    let third_msg = Message {
        owner: owner_b,
        ..Default::default()
    };

    // store the messages
    let first_id = MessageId::new([1; 32]);
    let _ = Storage::<MessageId, Message>::insert(&mut db, &first_id, &first_msg).unwrap();

    let second_id = MessageId::new([2; 32]);
    let _ = Storage::<MessageId, Message>::insert(&mut db, &second_id, &second_msg).unwrap();

    let third_id = MessageId::new([3; 32]);
    let _ = Storage::<MessageId, Message>::insert(&mut db, &third_id, &third_msg).unwrap();

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

    // get the messages from Owner B
    let result = client
        .messages(Some(&owner_b.to_string()), request.clone())
        .await
        .unwrap();

    // verify that Owner B has 1 message
    assert_eq!(result.results.len(), 1);
}

#[tokio::test]
async fn messages_empty_results_for_owner_with_no_messages() {
    let db = Database::default();
    let srv = FuelService::from_database(db.clone(), Config::local_node())
        .await
        .unwrap();
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
