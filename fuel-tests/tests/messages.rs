use fuel_core::{
    chain_config::{
        MessageConfig,
        StateConfig,
    },
    database::{
        storage::{
            FuelBlocks,
            Receipts,
        },
        Database,
    },
    model::FuelBlockDb,
    schema::scalars::TransactionId,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_interfaces::{
    common::{
        fuel_crypto::SecretKey,
        fuel_tx::{
            Receipt,
            TransactionBuilder,
        },
        fuel_types::Address,
    },
    db::{
        Messages,
        Transactions,
    },
    model::{
        DaBlockHeight,
        Message,
    },
};
use fuel_gql_client::{
    client::{
        FuelClient,
        PageDirection,
        PaginationRequest,
    },
    fuel_tx::Input,
    prelude::{
        Bytes32,
        Output,
        StorageAsMut,
        Transaction,
    },
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use rstest::rstest;

#[tokio::test]
async fn can_submit_genesis_message() {
    let mut rng = StdRng::seed_from_u64(1234);

    let secret_key: SecretKey = rng.gen();
    let pk = secret_key.public_key();

    let msg1 = MessageConfig {
        sender: rng.gen(),
        recipient: Input::owner(&pk),
        nonce: rng.gen(),
        amount: rng.gen(),
        data: vec![rng.gen()],
        da_height: DaBlockHeight(0),
    };
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .add_unsigned_message_input(
            secret_key,
            msg1.sender,
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
        recipient: owner_a,
        nonce: 1,
        ..Default::default()
    };
    let second_msg = MessageConfig {
        recipient: owner_a,
        nonce: 2,
        ..Default::default()
    };

    // create a message for owner B
    let third_msg = MessageConfig {
        recipient: owner_b,
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
        recipient: owner_a,
        nonce: 1,
        ..Default::default()
    };
    let second_msg = MessageConfig {
        recipient: owner_a,
        nonce: 2,
        ..Default::default()
    };

    // create a message for owner B
    let third_msg = MessageConfig {
        recipient: owner_b,
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
        assert_eq!(message.recipient.0 .0, owner_a)
    }

    // get the messages from Owner B
    let result = client
        .messages(Some(&owner_b.to_string()), request.clone())
        .await
        .unwrap();

    // verify that Owner B has 1 message
    assert_eq!(result.results.len(), 1);

    assert_eq!(result.results[0].recipient.0 .0, owner_b);
}

#[rstest]
#[tokio::test]
async fn messages_empty_results_for_owner_with_no_messages(
    #[values(PageDirection::Forward, PageDirection::Backward)] direction: PageDirection,
    #[values(Address::new([16; 32]), Address::new([0; 32]))] owner: Address,
) {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let request = PaginationRequest {
        cursor: None,
        results: 5,
        direction,
    };

    let result = client
        .messages(Some(&owner.to_string()), request)
        .await
        .unwrap();

    assert_eq!(result.results.len(), 0);
}

#[tokio::test]
async fn can_get_message_proof() {
    let transaction_id: TransactionId = Bytes32::default().into();
    let message_id: fuel_core::schema::scalars::MessageId =
        fuel_gql_client::fuel_types::MessageId::default().into();
    let block_id = Bytes32::default();

    let config = Config::local_node();
    let mut db = Database::default();

    db.storage::<Receipts>()
        .insert(
            &(transaction_id.into()),
            &[Receipt::message_out(
                message_id.into(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            )],
        )
        .unwrap();
    let outputs = vec![Output::Message {
        recipient: Default::default(),
        amount: Default::default(),
    }];
    let txn = Transaction::script(
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        outputs,
        Default::default(),
    );
    db.storage::<Transactions>()
        .insert(&(transaction_id.into()), &txn)
        .unwrap();
    db.update_tx_status(
        &(transaction_id.into()),
        fuel_core::tx_pool::TransactionStatus::Success {
            block_id,
            time: Default::default(),
            result: fuel_gql_client::state::ProgramState::Return(Default::default()),
        },
    )
    .unwrap();
    db.storage::<FuelBlocks>()
        .insert(
            &block_id,
            &FuelBlockDb {
                header: Default::default(),
                transactions: vec![transaction_id.into()],
            },
        )
        .unwrap();
    db.storage::<Messages>()
        .insert(&(message_id.into()), &Message::default())
        .unwrap();

    // setup server & client
    let srv = FuelService::from_database(db, config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let result = client
        .message_proof(
            transaction_id.to_string().as_str(),
            message_id.to_string().as_str(),
        )
        .await
        .unwrap()
        .unwrap();

    let mut tree = fuel_gql_client::fuel_merkle::binary::in_memory::MerkleTree::new();
    tree.push(fuel_gql_client::fuel_types::MessageId::from(message_id).as_ref());
    let (expected_root, expected_set) = tree.prove(0).unwrap();
    assert_eq!(
        *fuel_gql_client::fuel_types::Bytes32::from(result.proof_root),
        expected_root
    );
    let result_proof = result
        .proof_set
        .iter()
        .map(|p| *fuel_gql_client::fuel_types::Bytes32::from(p.clone()))
        .collect::<Vec<_>>();
    assert_eq!(result_proof, expected_set);
    assert_eq!(
        fuel_gql_client::fuel_types::MessageId::from(result.message.message_id),
        Message::default().id(),
    );
    assert_eq!(
        fuel_gql_client::fuel_types::Bytes32::from(
            result.block.transactions[0].id.clone()
        ),
        txn.id(),
    );
    assert_eq!(
        fuel_gql_client::fuel_types::Bytes64::from(result.signature),
        fuel_gql_client::fuel_types::Bytes64::default()
    );
}
