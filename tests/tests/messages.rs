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
    types::TransactionStatus,
    FuelClient,
    PageDirection,
    PaginationRequest,
};
use fuel_core_types::{
    fuel_asm::*,
    fuel_crypto::*,
    fuel_tx::*,
};
use rstest::rstest;

#[cfg(feature = "relayer")]
mod relayer;

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
    for n in [1, 2, 10] {
        let config = Config::local_node();
        let coin = config
            .chain_conf
            .initial_state
            .as_ref()
            .unwrap()
            .coins
            .as_ref()
            .unwrap()
            .first()
            .unwrap()
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

        let starting_offset = 32 + 8 + 8;

        let mut contract = vec![
            // Save the ptr to the script data to register 16.
            op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
            // Offset 16 by the length of bytes for the contract id
            // and two empty params. This will now point to the address
            // of the message recipient.
            op::addi(0x10, 0x10, starting_offset),
        ];
        contract.extend(args.iter().enumerate().flat_map(|(index, arg)| {
            [
                // The length of the message data in memory.
                op::movi(0x11, arg.message_data.len() as u32),
                // The index of the of the output message in the transactions outputs.
                op::movi(0x12, (index + 1) as u32),
                // The amount to send in coins.
                op::movi(0x13, 10),
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
        let contract_deploy = TransactionBuilder::create(bytecode, salt, vec![])
            .add_output(output)
            .finalize_as_transaction();

        let script_data = id
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
        }))
        .collect();

        // Call contract script.
        let script = vec![
            // Save the ptr to the script data to register 16.
            // This will be used to read the contract id + two
            // empty params. So 32 + 8 + 8.
            op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
            // Call the contract and forward no coins.
            op::call(0x10, RegId::ZERO, RegId::ZERO, RegId::CGAS),
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
            coin.asset_id,
            TxPointer::default(),
            Default::default(),
            predicate,
            vec![],
        );

        // Set the contract input because we are calling a contract.
        let inputs = vec![
            Input::Contract {
                utxo_id: UtxoId::new(Bytes32::zeroed(), 0),
                balance_root: Bytes32::zeroed(),
                state_root,
                tx_pointer: TxPointer::default(),
                contract_id: id,
            },
            coin_input,
        ];

        // The transaction will output a contract output and message output.
        let mut outputs = vec![Output::Contract {
            input_index: 0,
            balance_root: Bytes32::zeroed(),
            state_root: Bytes32::zeroed(),
        }];
        outputs.extend(
            args.iter()
                .map(|arg| Output::message(arg.recipient_address.into(), 10)),
        );

        // Create the contract calling script.
        let script = Transaction::script(
            Default::default(),
            1_000_000,
            Default::default(),
            script,
            script_data,
            inputs,
            outputs,
            vec![],
        );

        let transaction_id = script.id();

        // setup server & client
        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        // Deploy the contract.
        matches!(
            client.submit_and_await_commit(&contract_deploy).await,
            Ok(TransactionStatus::Success { .. })
        );

        // Call the contract.
        matches!(
            client.submit_and_await_commit(&script.into()).await,
            Ok(TransactionStatus::Success { .. })
        );

        // Get the receipts from the contract call.
        let receipts = client
            .receipts(transaction_id.to_string().as_str())
            .await
            .unwrap();

        // Get the message id from the receipts.
        let message_ids: Vec<_> = receipts
            .iter()
            .filter_map(|r| match r {
                Receipt::MessageOut { message_id, .. } => Some(*message_id),
                _ => None,
            })
            .collect();

        // Check we actually go the correct amount of ids back.
        assert_eq!(message_ids.len(), args.len(), "{receipts:?}");

        for message_id in message_ids.clone() {
            // Request the proof.
            let result = client
                .message_proof(
                    transaction_id.to_string().as_str(),
                    message_id.to_string().as_str(),
                )
                .await
                .unwrap()
                .unwrap();

            // 1. Generate the message id (message fields)
            // Produce message id.
            let generated_message_id = Output::message_id(
                &(result.sender.into()),
                &(result.recipient.into()),
                &(result.nonce.into()),
                result.amount.0,
                &result.data,
            );

            // Check message id is the same as the one passed in.
            assert_eq!(generated_message_id, message_id);

            // 2. Generate the block id. (full header)
            let mut hasher = Hasher::default();
            hasher.input(Bytes32::from(result.header.prev_root).as_ref());
            hasher
                .input(&u32::try_from(result.header.height.0).unwrap().to_be_bytes()[..]);
            hasher.input(result.header.time.0 .0.to_be_bytes());
            hasher.input(Bytes32::from(result.header.application_hash).as_ref());
            let block_id = hasher.digest();
            assert_eq!(block_id, Bytes32::from(result.header.id));

            // 3. Verify the proof. (message_id, proof set, root, index, num_message_ids)
            assert!(verify_merkle(
                result.header.output_messages_root.clone().into(),
                result.proof_index.0,
                result
                    .proof_set
                    .iter()
                    .cloned()
                    .map(Bytes32::from)
                    .collect(),
                result.header.output_messages_count.0,
                generated_message_id,
            ));

            // Generate a proof to compare
            let mut tree =
                fuel_core_types::fuel_merkle::binary::in_memory::MerkleTree::new();
            for id in &message_ids {
                tree.push(id.as_ref());
            }
            let (expected_root, expected_set) = tree.prove(result.proof_index.0).unwrap();

            let result_proof = result
                .proof_set
                .iter()
                .map(|p| *Bytes32::from(p.clone()))
                .collect::<Vec<_>>();
            assert_eq!(result_proof, expected_set);

            // Check the root matches the proof and the root on the header.
            assert_eq!(
                <[u8; 32]>::from(Bytes32::from(result.header.output_messages_root)),
                expected_root
            );

            // 4. Verify the signature. (block_id, signature)
            assert!(verify_signature(block_id.into(), result.signature));
        }
    }
}

// TODO: Others test:  Data missing etc.

fn verify_merkle(
    _root: Bytes32,
    _index: u64,
    _set: Vec<Bytes32>,
    _leaf_count: u64,
    _message_id: MessageId,
) -> bool {
    // TODO: Verify using merkle tree verify().
    true
}

fn verify_signature(
    block_id: fuel_core_types::blockchain::primitives::BlockId,
    signature: fuel_core_client::client::schema::Signature,
) -> bool {
    let signature = Signature::from(Bytes64::from(signature));
    let m = block_id.as_message();
    let public_key = signature.recover(m).unwrap();
    signature.verify(&public_key, m).is_ok()
}
