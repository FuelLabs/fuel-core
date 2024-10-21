use crate::helpers::TestContext;
use fuel_core::{
    chain_config::{
        ChainConfig,
        StateConfig,
    },
    schema::tx::receipt::all_receipts,
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
        StatusWithTransaction,
        TransactionStatus,
    },
    FuelClient,
};
use fuel_core_poa::{
    service::Mode,
    Trigger,
};
use fuel_core_types::{
    fuel_asm::{
        op,
        RegId,
    },
    fuel_crypto::SecretKey,
    fuel_tx::{
        field::ReceiptsRoot,
        Chargeable,
        *,
    },
    fuel_types::ChainId,
    fuel_vm::ProgramState,
    services::executor::TransactionExecutionResult,
    tai64::Tai64,
};
use futures::StreamExt;
use itertools::Itertools;
use rand::{
    prelude::StdRng,
    Rng,
    SeedableRng,
};
use std::{
    io::ErrorKind::NotFound,
    time::Duration,
};

mod predicates;
mod tx_pointer;
mod txn_status_subscription;
mod txpool;
mod upgrade;
mod utxo_validation;

#[test]
fn basic_script_snapshot() {
    // Since this script is referenced in docs, snapshot the byte representation in-case opcodes
    // are reassigned in the future
    let script = [
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();
    insta::assert_snapshot!(format!("{script:?}"));
}

#[tokio::test]
async fn dry_run_script() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let gas_limit = 1_000_000;
    let maturity = Default::default();

    let script = [
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    let tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_as_transaction();

    let tx_statuses = client.dry_run(&[tx.clone()]).await.unwrap();
    let log = tx_statuses
        .last()
        .expect("Nonempty response")
        .result
        .receipts();
    assert_eq!(3, log.len());

    assert!(matches!(log[0],
        Receipt::Log {
            ra, rb, ..
        } if ra == 0xca && rb == 0xba));

    assert!(matches!(log[1],
        Receipt::Return {
            val, ..
        } if val == 1));

    // ensure the tx isn't available in the blockchain history
    let err = client
        .transaction_status(&tx.id(&Default::default()))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), NotFound);
}

#[tokio::test]
async fn dry_run_create() {
    let mut rng = StdRng::seed_from_u64(2322);
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let salt: Salt = rng.gen();
    let contract_code = vec![];
    let contract = Contract::from(contract_code.clone());
    let root = contract.root();
    let state_root = Contract::default_state_root();
    let contract_id = contract.id(&salt, &root, &state_root);

    let tx = TransactionBuilder::create(contract_code.into(), salt, vec![])
        .add_fee_input()
        .add_output(Output::contract_created(contract_id, state_root))
        .finalize_as_transaction();

    let tx_statuses = client.dry_run(&[tx.clone()]).await.unwrap();
    assert_eq!(
        0,
        tx_statuses
            .last()
            .expect("Nonempty response")
            .result
            .receipts()
            .len()
    );

    // ensure the tx isn't available in the blockchain history
    let err = client
        .transaction_status(&tx.id(&Default::default()))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), NotFound);
}

#[tokio::test]
async fn dry_run_above_block_gas_limit() {
    let config = Config::local_node();
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // Given
    let gas_limit = client
        .consensus_parameters(0)
        .await
        .unwrap()
        .unwrap()
        .block_gas_limit();
    let maturity = Default::default();

    let script = [
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];
    let script = script.into_iter().collect();

    let tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_as_transaction();

    // When
    match client.dry_run(&[tx.clone()]).await {
        Ok(_) => panic!("Expected error"),
        Err(e) => assert_eq!(e.to_string(), "Response errors; The sum of the gas usable by the transactions is greater than the block gas limit".to_owned()),
    }
}

fn arb_large_script_tx<R: Rng + rand::CryptoRng>(
    max_fee_limit: Word,
    size: usize,
    rng: &mut R,
) -> Transaction {
    let mut script: Vec<_> = std::iter::repeat(op::noop()).take(size).collect();
    script.push(op::ret(RegId::ONE));
    let script_bytes = script.iter().flat_map(|op| op.to_bytes()).collect();
    let mut builder = TransactionBuilder::script(script_bytes, vec![]);
    let asset_id = *builder.get_params().base_asset_id();
    builder
        .max_fee_limit(max_fee_limit)
        .script_gas_limit(22430)
        .add_unsigned_coin_input(
            SecretKey::random(rng),
            rng.gen(),
            u32::MAX as u64,
            asset_id,
            Default::default(),
        )
        .finalize()
        .into()
}

fn config_with_size_limit(block_transaction_size_limit: u32) -> Config {
    let mut chain_config = ChainConfig::default();
    chain_config
        .consensus_parameters
        .set_block_transaction_size_limit(block_transaction_size_limit as u64)
        .expect("should be able to set the limit");
    let state_config = StateConfig::default();

    let mut config = Config::local_node_with_configs(chain_config, state_config);
    config.block_production = Trigger::Never;
    config
}

#[tokio::test]
async fn transaction_selector_can_saturate_block_according_to_block_transaction_size_limit(
) {
    let mut rng = rand::rngs::StdRng::from_entropy();

    // Create 5 transactions of increasing sizes.
    let arb_tx_count = 5;
    let transactions: Vec<(_, _)> = (0..arb_tx_count)
        .map(|i| {
            let script_bytes_count = 10_000 + (i * 100);
            let tx =
                arb_large_script_tx(189028 + i as Word, script_bytes_count, &mut rng);
            let size = tx
                .as_script()
                .expect("script tx expected")
                .metered_bytes_size() as u32;
            (tx, size)
        })
        .collect();

    // Run 5 cases. Each one will allow one more transaction to be included due to size.
    for n in 1..=arb_tx_count {
        // Calculate proper size limit for 'n' transactions
        let block_transaction_size_limit: u32 =
            transactions.iter().take(n).map(|(_, size)| size).sum();
        let config = config_with_size_limit(block_transaction_size_limit);
        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        // Submit all transactions
        for (tx, _) in &transactions {
            let status = client.submit(tx).await;
            assert!(status.is_ok())
        }

        // Produce a block.
        let block_height = client.produce_blocks(1, None).await.unwrap();

        // Assert that only 'n' first transactions (+1 for mint) were included.
        let block = client.block_by_height(block_height).await.unwrap().unwrap();
        assert_eq!(block.transactions.len(), n + 1);
        let expected_ids: Vec<_> = transactions
            .iter()
            .take(n)
            .map(|(tx, _)| tx.id(&ChainId::default()))
            .collect();
        let actual_ids: Vec<_> = block.transactions.into_iter().take(n).collect();
        assert_eq!(expected_ids, actual_ids);
    }
}

#[tokio::test]
async fn transaction_selector_can_select_a_transaction_that_fits_the_block_size_limit() {
    let mut rng = rand::rngs::StdRng::from_entropy();

    // Create 5 transactions of decreasing sizes.
    let arb_tx_count = 5;
    let transactions: Vec<(_, _)> = (0..arb_tx_count)
        .map(|i| {
            let script_bytes_count = 10_000 + (i * 100);
            let tx =
                arb_large_script_tx(189028 + i as Word, script_bytes_count, &mut rng);
            let size = tx
                .as_script()
                .expect("script tx expected")
                .metered_bytes_size() as u32;
            (tx, size)
        })
        .rev()
        .collect();

    let (smallest_tx, smallest_size) = transactions.last().unwrap();

    // Allow only the smallest transaction to fit.
    let config = config_with_size_limit(*smallest_size);
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // Submit all transactions
    for (tx, _) in &transactions {
        let status = client.submit(tx).await;
        assert!(status.is_ok())
    }

    // Produce a block.
    let block_height = client.produce_blocks(1, None).await.unwrap();

    // Assert that only the smallest transaction (+ mint) was included.
    let block = client.block_by_height(block_height).await.unwrap().unwrap();
    assert_eq!(block.transactions.len(), 2);
    let expected_id = smallest_tx.id(&ChainId::default());
    let actual_id = block.transactions.first().unwrap();
    assert_eq!(&expected_id, actual_id);
}

#[tokio::test]
async fn submit() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let gas_limit = 1_000_000;
    let maturity = Default::default();

    let script = [
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    let tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_as_transaction();

    client.submit_and_await_commit(&tx).await.unwrap();
    // verify that the tx returned from the api matches the submitted tx
    let ret_tx = client
        .transaction(&tx.id(&ChainId::default()))
        .await
        .unwrap()
        .unwrap()
        .transaction;
    assert_eq!(tx.id(&ChainId::default()), ret_tx.id(&ChainId::default()));
}

#[tokio::test]
async fn submit_and_await_status() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let gas_limit = 1_000_000;
    let maturity = Default::default();

    let script = [
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    let tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_as_transaction();

    let mut status_stream = client.submit_and_await_status(&tx).await.unwrap();
    let intermediate_status = status_stream.next().await.unwrap().unwrap();
    assert!(matches!(
        intermediate_status,
        TransactionStatus::Submitted { .. }
    ));
    let final_status = status_stream.next().await.unwrap().unwrap();
    assert!(matches!(final_status, TransactionStatus::Success { .. }));
}

#[tokio::test]
async fn dry_run_transaction_should_use_latest_block_time() {
    // Given
    let start_time = Tai64::from_unix(1337);
    let block_production_interval_seconds = 10;
    let number_of_blocks_to_produce_manually = 5;

    let mut config = Config::local_node();
    config.block_production = Trigger::Interval {
        block_time: Duration::from_secs(block_production_interval_seconds),
    };
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let gas_limit = 1_000_000;
    let maturity = Default::default();

    let get_block_timestamp_script =
        [op::bhei(0x10), op::time(0x11, 0x10), op::ret(0x11)];

    let script: Vec<u8> = get_block_timestamp_script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    let tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(gas_limit)
        .maturity(maturity)
        .add_fee_input()
        .finalize_as_transaction();

    // When
    client
        .produce_blocks(number_of_blocks_to_produce_manually, Some(start_time.0))
        .await
        .expect("failed to produce blocks");

    let TransactionExecutionResult::Success {
        result: Some(ProgramState::Return(returned_timestamp)),
        ..
    } = client
        .dry_run(&[tx.clone()])
        .await
        .expect("failed to dry run")[0]
        .result
    else {
        panic!("unexpected execution result from dry run")
    };

    // Then
    let expected_returned_timestamp = start_time.0
        + block_production_interval_seconds
            * number_of_blocks_to_produce_manually.saturating_sub(1) as u64;

    assert_eq!(expected_returned_timestamp, returned_timestamp);
}

#[ignore]
#[tokio::test]
async fn transaction_status_submitted() {
    // This test should ensure a transaction's status is Submitted while it is in the mempool
    // This test should also ensure a transaction's time of submission is correct in the returned status
    // Currently blocked until https://github.com/FuelLabs/fuel-core/issues/50 is resolved
    // as execution must be separate from submission for a tx to persist inside of the txpool
    // Merge with the submit_utxo_verified_tx test once utxo_verification is the default
    todo!();
}

#[tokio::test]
async fn receipts() {
    let transaction = Transaction::default_test_tx();
    let id = transaction.id(&ChainId::default());
    // setup server & client
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    // submit tx
    client
        .submit_and_await_commit(&transaction)
        .await
        .expect("transaction should insert");
    // run test
    let receipts = client.receipts(&id).await.unwrap();
    assert!(receipts.is_some());
}

#[tokio::test]
async fn receipts_decoding() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let actual_receipts = client.all_receipts().await.unwrap();
    assert_eq!(actual_receipts, all_receipts())
}

#[tokio::test]
async fn get_transaction_by_id() {
    // setup test data in the node
    let transaction = Transaction::default_test_tx();
    let id = transaction.id(&ChainId::default());

    // setup server & client
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    // submit tx to api
    client.submit_and_await_commit(&transaction).await.unwrap();

    // run test
    let transaction_response = client.transaction(&id).await.unwrap();
    assert!(transaction_response.is_some());
    if let Some(transaction_response) = transaction_response {
        assert!(matches!(
            transaction_response.status,
            TransactionStatus::Success { .. }
        ))
    }
}

#[tokio::test]
async fn get_transparent_transaction_by_id() {
    let transaction = Transaction::default_test_tx();
    let id = transaction.id(&ChainId::default());

    // setup server & client
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // submit tx
    let result = client.submit_and_await_commit(&transaction).await;
    assert!(result.is_ok());

    let opaque_tx = client
        .transaction(&id)
        .await
        .unwrap()
        .expect("expected some result")
        .transaction;

    // run test
    let transparent_transaction = client
        .transparent_transaction(&id)
        .await
        .unwrap()
        .expect("expected some value");

    // verify transaction round-trips via transparent graphql
    assert_eq!(opaque_tx, transparent_transaction);
}

#[tokio::test]
async fn get_executed_transaction_from_status() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // Given
    let transaction = Transaction::default_test_tx();
    let receipt_root_before_execution = *transaction.as_script().unwrap().receipts_root();
    assert_eq!(receipt_root_before_execution, Bytes32::zeroed());

    // When
    let result = client.submit_and_await_commit_with_tx(&transaction).await;

    // Then
    let status = result.expect("Expected executed transaction");
    let StatusWithTransaction::Success { transaction, .. } = status else {
        panic!("Not successful transaction")
    };
    let receipt_root_after_execution = *transaction.as_script().unwrap().receipts_root();
    assert_ne!(receipt_root_after_execution, Bytes32::zeroed());
}

#[tokio::test]
async fn get_transactions() {
    let alice = Address::from([1; 32]);
    let bob = Address::from([2; 32]);
    let charlie = Address::from([3; 32]);

    let mut context = TestContext::new(100).await;
    // Produce 10 blocks to ensure https://github.com/FuelLabs/fuel-core/issues/1825 is tested
    context
        .srv
        .shared
        .poa_adapter
        .manually_produce_blocks(
            None,
            Mode::Blocks {
                number_of_blocks: 10,
            },
        )
        .await
        .expect("Should produce block");
    let tx1 = context.transfer(alice, charlie, 1).await.unwrap();
    let tx2 = context.transfer(charlie, bob, 2).await.unwrap();
    let tx3 = context.transfer(bob, charlie, 3).await.unwrap();
    let tx4 = context.transfer(bob, charlie, 3).await.unwrap();
    let tx5 = context.transfer(charlie, alice, 1).await.unwrap();
    let tx6 = context.transfer(alice, charlie, 1).await.unwrap();

    // Skip the 10 first txs included in the tx pool by the creation of the 10 blocks above
    let page_request = PaginationRequest {
        cursor: None,
        results: 10,
        direction: PageDirection::Forward,
    };
    let client = context.client;
    let response = client.transactions(page_request.clone()).await.unwrap();
    assert_eq!(response.results.len(), 10);
    assert!(!response.has_previous_page);
    assert!(response.has_next_page);

    // Now, there are 12 transactions
    // [
    //  tx1, coinbase_tx1, tx2, coinbase_tx2, tx3, coinbase_tx3,
    //  tx4, coinbase_tx4, tx5, coinbase_tx5, tx6, coinbase_tx6,
    // ]

    // Query for first 6: [tx1, coinbase_tx1, tx2, coinbase_tx2, tx3, coinbase_tx3]
    let first_middle_page_request = PaginationRequest {
        cursor: response.cursor.clone(),
        results: 6,
        direction: PageDirection::Forward,
    };

    let response = client
        .transactions(first_middle_page_request.clone())
        .await
        .unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id(&ChainId::default()))
        .collect_vec();
    assert_eq!(transactions[0], tx1);
    // coinbase_tx1
    assert_eq!(transactions[2], tx2);
    // coinbase_tx2
    assert_eq!(transactions[4], tx3);
    // coinbase_tx3
    // Check pagination state for middle page
    assert!(response.has_next_page);
    assert!(response.has_previous_page);

    // Query for second page 2 with last given cursor: [tx4, coinbase_tx4, tx5, coinbase_tx5]
    let page_request_middle_page = PaginationRequest {
        cursor: response.cursor.clone(),
        results: 4,
        direction: PageDirection::Forward,
    };

    // Query backwards from last given cursor [3]: [tx3, coinbase_tx2, tx2, coinbase_tx1, tx1, tx_block_creation_10, tx_block_creation_9, ...]
    let page_request_backwards = PaginationRequest {
        cursor: response.cursor.clone(),
        results: 16,
        direction: PageDirection::Backward,
    };

    // Query forwards from last given cursor [3]: [tx4, coinbase_tx4, tx5, coinbase_tx5, tx6, coinbase_tx6]
    let page_request_forwards = PaginationRequest {
        cursor: response.cursor,
        results: 6,
        direction: PageDirection::Forward,
    };

    let response = client.transactions(page_request_middle_page).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id(&ChainId::default()))
        .collect_vec();
    // coinbase_tx4
    assert_eq!(transactions[0], tx4);
    // coinbase_tx5
    assert_eq!(transactions[2], tx5);
    // Check pagination state for middle page
    // it should have next and previous page
    assert!(response.has_next_page);
    assert!(response.has_previous_page);

    let response = client.transactions(page_request_backwards).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id(&ChainId::default()))
        .collect_vec();
    assert_eq!(transactions[0], tx3);
    // transactions[1] - coinbase_tx2
    assert_eq!(transactions[2], tx2);
    // transactions[3] - coinbase_tx1
    assert_eq!(transactions[4], tx1);
    // Check pagination state for last page
    assert!(!response.has_next_page);
    assert!(response.has_previous_page);

    let response = client.transactions(page_request_forwards).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id(&ChainId::default()))
        .collect_vec();
    assert_eq!(transactions[0], tx4);
    // coinbase_tx4
    assert_eq!(transactions[2], tx5);
    // coinbase_tx5
    assert_eq!(transactions[4], tx6);
    // coinbase_tx6
    // Check pagination state for last page
    assert!(!response.has_next_page);
    assert!(response.has_previous_page);
}

#[test_case::test_case(PageDirection::Forward; "forward")]
#[test_case::test_case(PageDirection::Backward; "backward")]
#[tokio::test]
async fn get_transactions_by_owner_returns_correct_number_of_results(
    direction: PageDirection,
) {
    let alice = Address::from([1; 32]);
    let bob = Address::from([2; 32]);

    let mut context = TestContext::new(100).await;
    let _ = context.transfer(alice, bob, 1).await.unwrap();
    let _ = context.transfer(alice, bob, 2).await.unwrap();
    let _ = context.transfer(alice, bob, 3).await.unwrap();
    let _ = context.transfer(alice, bob, 4).await.unwrap();
    let _ = context.transfer(alice, bob, 5).await.unwrap();

    let client = context.client;

    let all_transactions_forward = PaginationRequest {
        cursor: None,
        results: 10,
        direction,
    };
    let response = client
        .transactions_by_owner(&bob, all_transactions_forward)
        .await
        .unwrap();
    let transactions_forward = response
        .results
        .into_iter()
        .map(|tx| {
            assert!(matches!(tx.status, TransactionStatus::Success { .. }));
            tx.transaction
        })
        .collect_vec();
    assert_eq!(transactions_forward.len(), 5);
}

#[test_case::test_case(PageDirection::Forward; "forward")]
#[test_case::test_case(PageDirection::Backward; "backward")]
#[tokio::test]
async fn get_transactions_by_owner_supports_cursor(direction: PageDirection) {
    let alice = Address::from([1; 32]);
    let bob = Address::from([2; 32]);

    let mut context = TestContext::new(100).await;
    let _ = context.transfer(alice, bob, 1).await.unwrap();
    let _ = context.transfer(alice, bob, 2).await.unwrap();
    let _ = context.transfer(alice, bob, 3).await.unwrap();
    let _ = context.transfer(alice, bob, 4).await.unwrap();
    let _ = context.transfer(alice, bob, 5).await.unwrap();

    let client = context.client;

    let all_transactions_forward = PaginationRequest {
        cursor: None,
        results: 10,
        direction,
    };
    let response = client
        .transactions_by_owner(&bob, all_transactions_forward)
        .await
        .unwrap();
    let transactions_forward = response
        .results
        .into_iter()
        .map(|tx| {
            assert!(matches!(tx.status, TransactionStatus::Success { .. }));
            tx.transaction
        })
        .collect_vec();

    let forward_iter_three = PaginationRequest {
        cursor: None,
        results: 3,
        direction,
    };
    let response_after_iter_three = client
        .transactions_by_owner(&bob, forward_iter_three)
        .await
        .unwrap();
    let transactions_forward_iter_three = response_after_iter_three
        .results
        .into_iter()
        .map(|tx| {
            assert!(matches!(tx.status, TransactionStatus::Success { .. }));
            tx.transaction
        })
        .collect_vec();
    assert_eq!(transactions_forward_iter_three.len(), 3);
    assert_eq!(transactions_forward_iter_three[0], transactions_forward[0]);
    assert_eq!(transactions_forward_iter_three[1], transactions_forward[1]);
    assert_eq!(transactions_forward_iter_three[2], transactions_forward[2]);

    let forward_iter_next_two = PaginationRequest {
        cursor: response_after_iter_three.cursor.clone(),
        results: 2,
        direction,
    };
    let response = client
        .transactions_by_owner(&bob, forward_iter_next_two)
        .await
        .unwrap();
    let transactions_forward_iter_next_two = response
        .results
        .into_iter()
        .map(|tx| {
            assert!(matches!(tx.status, TransactionStatus::Success { .. }));
            tx.transaction
        })
        .collect_vec();
    assert_eq!(
        transactions_forward_iter_next_two[0],
        transactions_forward[3]
    );
    assert_eq!(
        transactions_forward_iter_next_two[1],
        transactions_forward[4]
    );
}

#[tokio::test]
async fn get_transactions_from_manual_blocks() {
    let context = TestContext::new(100).await;

    // create 10 txs
    let txs: Vec<_> = (0..10).map(create_mock_tx).collect();

    // make 1st test block
    let first_batch = txs.iter().take(5).cloned().collect();
    context
        .srv
        .shared
        .poa_adapter
        .manually_produce_blocks(None, Mode::BlockWithTransactions(first_batch))
        .await
        .expect("Should produce first block with first 5 transactions.");

    // make 2nd test block
    let second_batch = txs.iter().skip(5).take(5).cloned().collect();
    context
        .srv
        .shared
        .poa_adapter
        .manually_produce_blocks(None, Mode::BlockWithTransactions(second_batch))
        .await
        .expect("Should produce block with last 5 transactions.");

    // Query for first 4: [0, 1, 2, 3]
    let page_request_forwards = PaginationRequest {
        cursor: None,
        results: 4,
        direction: PageDirection::Forward,
    };
    let response = context
        .client
        .transactions(page_request_forwards)
        .await
        .unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id(&ChainId::default()))
        .collect_vec();
    assert_eq!(transactions[0], txs[0].id(&ChainId::default()));
    assert_eq!(transactions[1], txs[1].id(&ChainId::default()));
    assert_eq!(transactions[2], txs[2].id(&ChainId::default()));
    assert_eq!(transactions[3], txs[3].id(&ChainId::default()));

    // Query forwards from last given cursor [2]: [4, coinbase_tx1, 5, 6, coinbase_tx2]
    let next_page_request_forwards = PaginationRequest {
        cursor: response.cursor,
        results: 5,
        direction: PageDirection::Forward,
    };
    let response = context
        .client
        .transactions(next_page_request_forwards)
        .await
        .unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id(&ChainId::default()))
        .collect_vec();
    assert_eq!(transactions[0], txs[4].id(&ChainId::default()));
    // coinbase_tx1
    assert_eq!(transactions[2], txs[5].id(&ChainId::default()));
    assert_eq!(transactions[3], txs[6].id(&ChainId::default()));
    // coinbase_tx2

    // Query backwards from last given cursor [8]: [6, 5, coinbase_tx1, 4, 3, 2, 1, 0]
    let page_request_backwards = PaginationRequest {
        cursor: response.cursor,
        results: 10,
        direction: PageDirection::Backward,
    };
    let response = context
        .client
        .transactions(page_request_backwards)
        .await
        .unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id(&ChainId::default()))
        .collect_vec();
    assert_eq!(transactions[0], txs[6].id(&ChainId::default()));
    assert_eq!(transactions[1], txs[5].id(&ChainId::default()));
    // transactions[2] coinbase_tx1
    assert_eq!(transactions[3], txs[4].id(&ChainId::default()));
    assert_eq!(transactions[4], txs[3].id(&ChainId::default()));
    assert_eq!(transactions[5], txs[2].id(&ChainId::default()));
    assert_eq!(transactions[6], txs[1].id(&ChainId::default()));
    assert_eq!(transactions[7], txs[0].id(&ChainId::default()));
}

#[tokio::test]
async fn get_owned_transactions() {
    let alice = Address::from([1; 32]);
    let bob = Address::from([2; 32]);
    let charlie = Address::from([3; 32]);

    let mut context = TestContext::new(100).await;
    let tx1 = context.transfer(alice, charlie, 1).await.unwrap();
    let tx2 = context.transfer(charlie, bob, 2).await.unwrap();
    let tx3 = context.transfer(bob, charlie, 3).await.unwrap();

    // Query for transactions by owner, for each owner respectively
    let client = context.client;
    let page_request = PaginationRequest {
        cursor: None,
        results: 5,
        direction: PageDirection::Forward,
    };
    let alice_txs = client
        .transactions_by_owner(&alice, page_request.clone())
        .await
        .unwrap()
        .results
        .iter()
        .map(|tx| tx.transaction.id(&ChainId::default()))
        .collect_vec();

    let bob_txs = client
        .transactions_by_owner(&bob, page_request.clone())
        .await
        .unwrap()
        .results
        .iter()
        .map(|tx| tx.transaction.id(&ChainId::default()))
        .collect_vec();

    let charlie_txs = client
        .transactions_by_owner(&charlie, page_request.clone())
        .await
        .unwrap()
        .results
        .iter()
        .map(|tx| tx.transaction.id(&ChainId::default()))
        .collect_vec();

    assert_eq!(&alice_txs, &[tx1]);
    assert_eq!(&bob_txs, &[tx2, tx3]);
    assert_eq!(&charlie_txs, &[tx1, tx2, tx3]);
}

// add random val for unique tx
fn create_mock_tx(val: u64) -> Transaction {
    let mut rng = StdRng::seed_from_u64(val);

    TransactionBuilder::script(val.to_be_bytes().to_vec(), Default::default())
        .add_unsigned_coin_input(
            SecretKey::random(&mut rng),
            rng.gen(),
            1_000_000,
            Default::default(),
            Default::default(),
        )
        .finalize_as_transaction()
}
