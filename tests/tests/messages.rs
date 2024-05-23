#![allow(non_snake_case)]

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
use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    types::{
        message::MessageStatus,
        TransactionStatus,
    },
    FuelClient,
};
use fuel_core_storage::tables::Coins;
use fuel_core_types::{
    fuel_asm::{
        op,
        GTFArgs,
        RegId,
    },
    fuel_crypto::*,
    fuel_merkle,
    fuel_tx::{
        input::message::compute_message_id,
        Word,
        *,
    },
    fuel_types::ChainId,
};
use itertools::Itertools;
use rstest::rstest;
use std::ops::Deref;

#[cfg(feature = "relayer")]
mod relayer;

fn setup_config(messages: impl IntoIterator<Item = MessageConfig>) -> Config {
    let state = StateConfig {
        messages: messages.into_iter().collect_vec(),
        ..Default::default()
    };

    Config::local_node_with_state_config(state)
}

#[tokio::test]
async fn messages_returns_messages_for_all_owners() {
    // create some owners
    let owner_a = Address::new([1; 32]);
    let owner_b = Address::new([2; 32]);

    // create some messages for owner A
    let first_msg = MessageConfig {
        recipient: owner_a,
        nonce: 1.into(),
        ..Default::default()
    };
    let second_msg = MessageConfig {
        recipient: owner_a,
        nonce: 2.into(),
        ..Default::default()
    };

    // create a message for owner B
    let third_msg = MessageConfig {
        recipient: owner_b,
        nonce: 3.into(),
        ..Default::default()
    };

    // configure the messages
    let config = setup_config(vec![first_msg, second_msg, third_msg]);

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
    let owner_c = Address::new([3; 32]);

    // create some messages for owner A
    let first_msg = MessageConfig {
        recipient: owner_a,
        nonce: 1.into(),
        ..Default::default()
    };
    let second_msg = MessageConfig {
        recipient: owner_a,
        nonce: 2.into(),
        ..Default::default()
    };

    // create a message for owner B
    let third_msg = MessageConfig {
        recipient: owner_b,
        nonce: 3.into(),
        ..Default::default()
    };

    let config = setup_config(vec![first_msg, second_msg, third_msg]);

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
        .messages(Some(&owner_a), request.clone())
        .await
        .unwrap();

    // verify that Owner A has 2 messages
    assert_eq!(result.results.len(), 2);

    // verify messages owner matches
    for message in result.results {
        assert_eq!(message.recipient, owner_a)
    }

    // get the messages from Owner B
    let result = client
        .messages(Some(&owner_b), request.clone())
        .await
        .unwrap();

    // verify that Owner B has 1 message
    assert_eq!(result.results.len(), 1);

    let recipient: Address = result.results[0].recipient;
    assert_eq!(recipient, owner_b);

    // get the messages from Owner C
    let result = client
        .messages(Some(&owner_c), request.clone())
        .await
        .unwrap();

    // verify that Owner C has no messages
    assert_eq!(result.results.len(), 0);
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

    let result = client.messages(Some(&owner), request).await.unwrap();

    assert_eq!(result.results.len(), 0);
}

#[tokio::test]
async fn message_status__can_get_unspent() {
    // Given
    let owner = Address::new([1; 32]);
    let nonce = 1.into();
    let amount = 1_000;

    let msg = MessageConfig {
        recipient: owner,
        nonce: 1.into(),
        amount,
        ..Default::default()
    };

    let config = setup_config(vec![msg]);

    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // When
    let status = client.message_status(&nonce).await.unwrap();

    // Then
    assert_eq!(status, MessageStatus::Unspent);
}

#[tokio::test]
async fn message_status__can_get_spent() {
    // Given
    let msg_recipient = Address::from([1; 32]);
    let output_recipient = Address::from([2; 32]);
    let msg_sender = Address::from([3; 32]);

    let nonce = 1.into();
    let amount = 1_000;

    let msg = MessageConfig {
        sender: msg_sender,
        recipient: msg_recipient,
        nonce,
        amount,
        ..Default::default()
    };

    let config = setup_config(vec![msg]);

    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let input = Input::message_coin_signed(
        msg_sender,
        msg_recipient,
        amount,
        nonce,
        Default::default(),
    );

    let output = Output::coin(output_recipient, amount, Default::default());

    let tx = Transaction::script(
        1_000_000,
        vec![],
        vec![],
        policies::Policies::new().with_max_fee(0),
        vec![input],
        vec![output],
        vec![Vec::new().into()],
    )
    .into();

    // When
    client.submit_and_await_commit(&tx).await.unwrap();
    let status = client.message_status(&nonce).await.unwrap();

    // Then
    assert_eq!(status, MessageStatus::Spent);
}

#[tokio::test]
async fn message_status__can_get_notfound() {
    // Given
    let nonce = 1.into();

    let config = Config::local_node();

    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // When
    let status = client.message_status(&nonce).await.unwrap();

    // Then
    assert_eq!(status, MessageStatus::NotFound);
}

#[tokio::test]
async fn can_get_message_proof() {
    for n in [1, 2, 10] {
        let config = Config::local_node();

        let coin = config
            .snapshot_reader
            .read::<Coins>()
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
            .unwrap()[0]
            .value
            .clone();

        struct MessageArgs {
            recipient_address: [u8; 32],
            message_data: Vec<u8>,
        }

        let args: Vec<_> = (0..n)
            .map(|i| MessageArgs {
                recipient_address: [i + 1; 32],
                message_data: i.to_be_bytes().into(),
            })
            .collect();

        let amount = 10;
        let starting_offset = 32 + 8 + 8;

        let mut contract = vec![
            // Save the ptr to the script data to register 16.
            op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
            // Offset 16 by the length of bytes for the contract id
            // and two empty params. This will now point to the address
            // of the message recipient.
            op::addi(0x10, 0x10, starting_offset),
        ];
        contract.extend(args.iter().flat_map(|arg| {
            [
                // Pointer to the message in memory
                op::addi(0x11, 0x10, 32),
                // The length of the message data in memory.
                op::movi(0x12, arg.message_data.len() as u32),
                // The amount to send in coins.
                op::movi(0x13, amount),
                // Send the message output.
                op::smo(0x10, 0x11, 0x12, 0x13),
                // Offset to the next recipient address (this recipient address + message data len)
                op::addi(0x10, 0x10, 32 + arg.message_data.len() as u16),
            ]
        }));
        // Return.
        contract.push(op::ret(RegId::ONE));

        // Contract code.
        let bytecode: Witness = contract.into_iter().collect::<Vec<u8>>().into();

        // Setup the contract.
        let salt = Salt::zeroed();
        let contract = Contract::from(bytecode.as_ref());
        let root = contract.root();
        let state_root = Contract::initial_state_root(std::iter::empty());
        let id = contract.id(&salt, &root, &state_root);
        let output = Output::contract_created(id, state_root);

        // Create the contract deploy transaction.
        let mut contract_deploy = TransactionBuilder::create(bytecode, salt, vec![])
            .add_random_fee_input()
            .add_output(output)
            .finalize_as_transaction();

        let smo_data: Vec<_> = id
            .iter()
            .copied()
            // Empty Param 1
            .chain((0 as Word).to_be_bytes().iter().copied())
            // Empty Param 2
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain(args.iter().flat_map(|arg| {
                // Recipient address
                arg.recipient_address.into_iter()
                    // The message data
                    .chain(arg.message_data.clone().into_iter())
            })).collect();
        let script_data = AssetId::BASE
            .into_iter()
            .chain(smo_data.into_iter())
            .collect();

        // Call contract script.
        // Save the ptr to the script data to register 16.
        // This will be used to read the contract id + two
        // empty params. So 32 + 8 + 8.
        let script = [
            op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
            // load balance to forward to 0x11
            op::movi(0x11, n as u32 * amount),
            // shift the smo data into 0x10
            op::addi(0x12, 0x10, AssetId::LEN as u16),
            // Call the contract and forward no coins.
            op::call(0x12, 0x11, 0x10, RegId::CGAS),
            // Return.
            op::ret(RegId::ONE),
        ];
        let script: Vec<u8> = script
            .iter()
            .flat_map(|op| u32::from(*op).to_be_bytes())
            .collect();

        let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
        let owner = Input::predicate_owner(&predicate);
        let coin_input = Input::coin_predicate(
            Default::default(),
            owner,
            1000,
            *coin.asset_id(),
            TxPointer::default(),
            Default::default(),
            predicate,
            vec![],
        );

        // Set the contract input because we are calling a contract.
        let inputs = vec![
            Input::contract(
                UtxoId::new(Bytes32::zeroed(), 0),
                Bytes32::zeroed(),
                state_root,
                TxPointer::default(),
                id,
            ),
            coin_input,
        ];

        // The transaction will output a contract output and message output.
        let outputs = vec![Output::contract(0, Bytes32::zeroed(), Bytes32::zeroed())];

        // Create the contract calling script.
        let script = Transaction::script(
            1_000_000,
            script,
            script_data,
            policies::Policies::new().with_max_fee(0),
            inputs,
            outputs,
            vec![],
        );

        let transaction_id = script.id(&ChainId::default());

        // setup server & client
        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        client
            .estimate_predicates(&mut contract_deploy)
            .await
            .expect("Should be able to estimate deploy tx");

        // Deploy the contract.
        matches!(
            client.submit_and_await_commit(&contract_deploy).await,
            Ok(TransactionStatus::Success { .. })
        );

        let mut script = script.into();
        client
            .estimate_predicates(&mut script)
            .await
            .expect("Should be able to estimate script tx");
        // Call the contract.
        matches!(
            client.submit_and_await_commit(&script).await,
            Ok(TransactionStatus::Success { .. })
        );

        // Produce one more block, because we can't create proof for the last block.
        let last_height = client.produce_blocks(1, None).await.unwrap();

        // Get the receipts from the contract call.
        let receipts = client.receipts(&transaction_id).await.unwrap().unwrap();

        // Get the message id from the receipts.
        let message_ids: Vec<_> =
            receipts.iter().filter_map(|r| r.message_id()).collect();

        // Get the nonces from the receipt
        let nonces: Vec<_> = receipts.iter().filter_map(|r| r.nonce()).collect();

        // Check we actually go the correct amount of ids back.
        assert_eq!(nonces.len(), args.len(), "{receipts:?}");

        for nonce in nonces.clone() {
            // Request the proof.
            let result = client
                .message_proof(&transaction_id, nonce, None, Some(last_height))
                .await
                .unwrap()
                .unwrap();

            // 1. Generate the message id (message fields)
            // Produce message id.
            let generated_message_id = compute_message_id(
                &result.sender,
                &result.recipient,
                &result.nonce,
                result.amount,
                &result.data,
            );

            // 2. Generate the block id. (full header)
            let mut hasher = Hasher::default();
            hasher.input(result.message_block_header.prev_root.as_ref());
            hasher.input(&result.message_block_header.height.to_be_bytes()[..]);
            hasher.input(result.message_block_header.time.0.to_be_bytes());
            hasher.input(result.message_block_header.application_hash.as_ref());
            let message_block_id = hasher.digest();
            assert_eq!(message_block_id, result.message_block_header.id);

            // 3. Verify the message proof. (message receipt root, message id, proof index, proof set, num message receipts in the block)
            let message_proof_index = result.message_proof.proof_index;
            let message_proof_set: Vec<_> = result
                .message_proof
                .proof_set
                .iter()
                .cloned()
                .map(Bytes32::from)
                .collect();
            assert!(verify_merkle(
                result.message_block_header.message_outbox_root,
                &generated_message_id,
                message_proof_index,
                &message_proof_set,
                result.message_block_header.message_receipt_count as u64,
            ));

            // Generate a proof to compare
            let mut tree = fuel_merkle::binary::in_memory::MerkleTree::new();
            for id in &message_ids {
                tree.push(id.as_ref());
            }
            let (expected_root, expected_set) = tree.prove(message_proof_index).unwrap();
            let expected_set: Vec<_> =
                expected_set.into_iter().map(Bytes32::from).collect();

            assert_eq!(message_proof_set, expected_set);

            // Check the root matches the proof and the root on the header.
            assert_eq!(
                <[u8; 32]>::from(result.message_block_header.message_outbox_root),
                expected_root
            );

            // 4. Verify the block proof. (prev_root, block id, proof index, proof set, block count)
            let block_proof_index = result.block_proof.proof_index;
            let block_proof_set: Vec<_> = result
                .block_proof
                .proof_set
                .iter()
                .cloned()
                .map(Bytes32::from)
                .collect();
            let blocks_count = result.commit_block_header.height;
            assert!(verify_merkle(
                result.commit_block_header.prev_root,
                &message_block_id,
                block_proof_index,
                &block_proof_set,
                blocks_count as u64,
            ));
        }
    }
}

// TODO: Others test:  Data missing etc.
fn verify_merkle<D: AsRef<[u8]>>(
    root: Bytes32,
    data: &D,
    index: u64,
    set: &[Bytes32],
    leaf_count: u64,
) -> bool {
    let set: Vec<_> = set.iter().map(|bytes| *bytes.deref()).collect();
    fuel_merkle::binary::verify(root.deref(), data, &set, index, leaf_count)
}

#[tokio::test]
async fn can_get_message() {
    // create an owner
    let owner = Address::new([1; 32]);

    // create some messages for the owner
    let first_msg = MessageConfig {
        recipient: owner,
        nonce: 1.into(),
        ..Default::default()
    };

    // configure the messages
    let state_config = StateConfig {
        messages: vec![first_msg.clone()],
        ..Default::default()
    };
    let config = Config::local_node_with_state_config(state_config);

    // setup service and client
    let service = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(service.bound_address);

    // run test
    let message_response = client.message(&first_msg.nonce).await.unwrap();
    assert!(message_response.is_some());
    if let Some(message_response) = message_response {
        assert_eq!(message_response.nonce, first_msg.nonce);
    }
}

#[tokio::test]
async fn can_get_empty_message() {
    let config = Config::local_node_with_state_config(StateConfig::default());

    let service = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(service.bound_address);

    let message_response = client.message(&1.into()).await.unwrap();
    assert!(message_response.is_none());
}
