use fuel_core::{config::Config, database::Database, service::FuelService};
use fuel_core_interfaces::common::fuel_storage::Storage;
use fuel_core_interfaces::model::Message;
use fuel_crypto::fuel_types::{Address, MessageId};
use fuel_gql_client::client::{FuelClient, PageDirection, PaginationRequest};

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
