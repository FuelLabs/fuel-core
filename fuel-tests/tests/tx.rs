use crate::helpers::TestContext;
use fuel_core::{
    database::Database,
    executor::Executor,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_interfaces::{
    common::{
        fuel_tx,
        fuel_vm::{
            consts::*,
            prelude::*,
        },
        tai64::Tai64,
    },
    executor::ExecutionBlock,
    model::{
        FuelConsensusHeader,
        PartialFuelBlock,
        PartialFuelBlockHeader,
    },
};
use fuel_gql_client::client::{
    types::TransactionStatus,
    FuelClient,
    PageDirection,
    PaginationRequest,
};
use itertools::Itertools;
use rand::{
    prelude::StdRng,
    Rng,
    SeedableRng,
};
use std::{
    io,
    io::ErrorKind::NotFound,
};

mod predicates;
mod txn_status_subscription;
mod utxo_validation;

#[test]
fn basic_script_snapshot() {
    // Since this script is referenced in docs, snapshot the byte representation in-case opcodes
    // are reassigned in the future
    let script = vec![
        Opcode::ADDI(0x10, REG_ZERO, 0xca),
        Opcode::ADDI(0x11, REG_ZERO, 0xba),
        Opcode::LOG(0x10, 0x11, REG_ZERO, REG_ZERO),
        Opcode::RET(REG_ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();
    insta::assert_snapshot!(format!("{:?}", script));
}

#[tokio::test]
async fn dry_run_script() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let gas_price = 0;
    let gas_limit = 1_000_000;
    let maturity = 0;

    let script = vec![
        Opcode::ADDI(0x10, REG_ZERO, 0xca),
        Opcode::ADDI(0x11, REG_ZERO, 0xba),
        Opcode::LOG(0x10, 0x11, REG_ZERO, REG_ZERO),
        Opcode::RET(REG_ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    let tx = fuel_tx::Transaction::script(
        gas_price,
        gas_limit,
        maturity,
        script,
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .into();

    let log = client.dry_run(&tx).await.unwrap();
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
        .transaction_status(&format!("{:#x}", tx.id()))
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
    let contract = fuel_tx::Contract::from(contract_code.clone());
    let root = contract.root();
    let state_root = fuel_tx::Contract::default_state_root();
    let contract_id = contract.id(&salt, &root, &state_root);

    let tx = Transaction::create(
        0,
        0,
        0,
        0,
        salt,
        vec![],
        vec![],
        vec![Output::contract_created(contract_id, state_root)],
        vec![contract_code.into()],
    )
    .into();

    let receipts = client.dry_run(&tx).await.unwrap();
    assert_eq!(0, receipts.len());

    // ensure the tx isn't available in the blockchain history
    let err = client
        .transaction_status(&format!("{:#x}", tx.id()))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), NotFound);
}

#[tokio::test]
async fn submit() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let gas_price = 0;
    let gas_limit = 1_000_000;
    let maturity = 0;

    let script = vec![
        Opcode::ADDI(0x10, REG_ZERO, 0xca),
        Opcode::ADDI(0x11, REG_ZERO, 0xba),
        Opcode::LOG(0x10, 0x11, REG_ZERO, REG_ZERO),
        Opcode::RET(REG_ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    let tx = fuel_tx::Transaction::script(
        gas_price,
        gas_limit,
        maturity,
        script,
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .into();

    client.submit_and_await_commit(&tx).await.unwrap();
    // verify that the tx returned from the api matches the submitted tx
    let ret_tx = client
        .transaction(&tx.id().to_string())
        .await
        .unwrap()
        .unwrap()
        .transaction;
    assert_eq!(tx.id(), ret_tx.id());
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
    let transaction = fuel_tx::Transaction::default();
    let id = transaction.id();
    // setup server & client
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    // submit tx
    client
        .submit_and_await_commit(&transaction)
        .await
        .expect("transaction should insert");
    // run test
    let receipts = client.receipts(&format!("{:#x}", id)).await.unwrap();
    assert!(!receipts.is_empty());
}

#[tokio::test]
async fn get_transaction_by_id() {
    // setup test data in the node
    let transaction = fuel_tx::Transaction::default();
    let id = transaction.id();

    // setup server & client
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    // submit tx to api
    client.submit_and_await_commit(&transaction).await.unwrap();

    // run test
    let transaction_response = client.transaction(&format!("{:#x}", id)).await.unwrap();
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
    let transaction = fuel_tx::Transaction::default();
    let id = transaction.id();

    // setup server & client
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // submit tx
    let result = client.submit_and_await_commit(&transaction).await;
    assert!(result.is_ok());

    let opaque_tx = client
        .transaction(&format!("{:#x}", id))
        .await
        .unwrap()
        .expect("expected some result")
        .transaction;

    // run test
    let transparent_transaction = client
        .transparent_transaction(&format!("{:#x}", id))
        .await
        .unwrap()
        .expect("expected some value");

    // verify transaction round-trips via transparent graphql
    assert_eq!(opaque_tx, transparent_transaction);
}

#[tokio::test]
async fn get_transactions() {
    let alice = Address::from([0; 32]);
    let bob = Address::from([1; 32]);
    let charlie = Address::from([2; 32]);

    let mut context = TestContext::new(100).await;
    let tx1 = context.transfer(alice, charlie, 1).await.unwrap();
    let tx2 = context.transfer(charlie, bob, 2).await.unwrap();
    let tx3 = context.transfer(bob, charlie, 3).await.unwrap();
    let tx4 = context.transfer(bob, charlie, 3).await.unwrap();
    let tx5 = context.transfer(charlie, alice, 1).await.unwrap();
    let tx6 = context.transfer(alice, charlie, 1).await.unwrap();

    // there are 12 transactions
    // [
    //  coinbase_tx1, tx1, coinbase_tx2, tx2, coinbase_tx3, tx3,
    //  coinbase_tx4, tx4, coinbase_tx5, tx5, coinbase_tx6, tx6
    // ]

    // Query for first 6: [coinbase_tx1, tx1, coinbase_tx2, tx2, coinbase_tx3, tx3]
    let client = context.client;
    let page_request = PaginationRequest {
        cursor: None,
        results: 6,
        direction: PageDirection::Forward,
    };

    let response = client.transactions(page_request.clone()).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();
    // coinbase_tx1
    assert_eq!(transactions[1], tx1);
    // coinbase_tx2
    assert_eq!(transactions[3], tx2);
    // coinbase_tx3
    assert_eq!(transactions[5], tx3);
    // Check pagination state for first page
    assert!(response.has_next_page);
    assert!(!response.has_previous_page);

    // Query for second page 2 with last given cursor: [coinbase_tx4, tx4, coinbase_tx5, tx5]
    let page_request_middle_page = PaginationRequest {
        cursor: response.cursor.clone(),
        results: 4,
        direction: PageDirection::Forward,
    };

    // Query backwards from last given cursor [3]: [coinbase_tx3, tx2, coinbase_tx2, tx1, coinbase_tx1]
    let page_request_backwards = PaginationRequest {
        cursor: response.cursor.clone(),
        results: 6,
        direction: PageDirection::Backward,
    };

    // Query forwards from last given cursor [3]: [coinbase_tx4, tx4, coinbase_tx5, tx5, coinbase_tx6, tx6]
    let page_request_forwards = PaginationRequest {
        cursor: response.cursor,
        results: 6,
        direction: PageDirection::Forward,
    };

    let response = client.transactions(page_request_middle_page).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();
    // coinbase_tx4
    assert_eq!(transactions[1], tx4);
    // coinbase_tx5
    assert_eq!(transactions[3], tx5);
    // Check pagination state for middle page
    // it should have next and previous page
    assert!(response.has_next_page);
    assert!(response.has_previous_page);

    let response = client.transactions(page_request_backwards).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();
    // transactions[0] - coinbase_tx3
    assert_eq!(transactions[1], tx2);
    // transactions[2] - coinbase_tx2
    assert_eq!(transactions[3], tx1);
    // transactions[4] - coinbase_tx1
    // Check pagination state for last page
    assert!(!response.has_next_page);
    assert!(response.has_previous_page);

    let response = client.transactions(page_request_forwards).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();
    // coinbase_tx4
    assert_eq!(transactions[1], tx4);
    // coinbase_tx5
    assert_eq!(transactions[3], tx5);
    // coinbase_tx6
    assert_eq!(transactions[5], tx6);
    // Check pagination state for last page
    assert!(!response.has_next_page);
    assert!(response.has_previous_page);
}

#[tokio::test]
async fn get_transactions_from_manual_blocks() {
    let (executor, db) = get_executor_and_db();
    // get access to a client
    let client = initialize_client(db).await;

    // create 10 txs
    let txs: Vec<Transaction> = (0..10).map(create_mock_tx).collect();

    // make 1st test block
    let first_test_block = PartialFuelBlock {
        header: PartialFuelBlockHeader {
            consensus: FuelConsensusHeader {
                height: 1u32.into(),
                time: Tai64::now(),
                ..Default::default()
            },
            ..Default::default()
        },

        // set the first 5 ids of the manually saved txs
        transactions: txs.iter().take(5).cloned().collect(),
    };

    // make 2nd test block
    let second_test_block = PartialFuelBlock {
        header: PartialFuelBlockHeader {
            consensus: FuelConsensusHeader {
                height: 2u32.into(),
                time: Tai64::now(),
                ..Default::default()
            },
            ..Default::default()
        },
        // set the last 5 ids of the manually saved txs
        transactions: txs.iter().skip(5).take(5).cloned().collect(),
    };

    // process blocks and save block height
    executor
        .execute_and_commit(ExecutionBlock::Production(first_test_block))
        .unwrap();
    executor
        .execute_and_commit(ExecutionBlock::Production(second_test_block))
        .unwrap();

    // Query for first 4: [coinbase_tx1, 0, 1, 2]
    let page_request_forwards = PaginationRequest {
        cursor: None,
        results: 4,
        direction: PageDirection::Forward,
    };
    let response = client.transactions(page_request_forwards).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();
    // coinbase_tx1
    assert_eq!(transactions[1], txs[0].id());
    assert_eq!(transactions[2], txs[1].id());
    assert_eq!(transactions[3], txs[2].id());

    // Query forwards from last given cursor [2]: [3, 4, coinbase_tx2, 5, 6]
    let next_page_request_forwards = PaginationRequest {
        cursor: response.cursor,
        results: 5,
        direction: PageDirection::Forward,
    };
    let response = client
        .transactions(next_page_request_forwards)
        .await
        .unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();
    assert_eq!(transactions[0], txs[3].id());
    assert_eq!(transactions[1], txs[4].id());
    // coinbase_tx2
    assert_eq!(transactions[3], txs[5].id());
    assert_eq!(transactions[4], txs[6].id());

    // Query backwards from last given cursor [8]: [coinbase_tx1, 0, 1, 2, 3, 4, coinbase_tx2, 5]
    let page_request_backwards = PaginationRequest {
        cursor: response.cursor,
        results: 10,
        direction: PageDirection::Backward,
    };
    let response = client.transactions(page_request_backwards).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();
    assert_eq!(transactions[0], txs[5].id());
    // transactions[1] coinbase_tx2
    assert_eq!(transactions[2], txs[4].id());
    assert_eq!(transactions[3], txs[3].id());
    assert_eq!(transactions[4], txs[2].id());
    assert_eq!(transactions[5], txs[1].id());
    assert_eq!(transactions[6], txs[0].id());
    // transactions[7] coinbase_tx1
}

#[tokio::test]
async fn get_owned_transactions() {
    let alice = Address::from([0; 32]);
    let bob = Address::from([1; 32]);
    let charlie = Address::from([2; 32]);

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
        .transactions_by_owner(&format!("{:#x}", alice), page_request.clone())
        .await
        .unwrap()
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();

    let bob_txs = client
        .transactions_by_owner(&format!("{:#x}", bob), page_request.clone())
        .await
        .unwrap()
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();

    let charlie_txs = client
        .transactions_by_owner(&format!("{:#x}", charlie), page_request.clone())
        .await
        .unwrap()
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();

    assert_eq!(&alice_txs, &[tx1]);
    assert_eq!(&bob_txs, &[tx2, tx3]);
    assert_eq!(&charlie_txs, &[tx1, tx2, tx3]);
}

impl TestContext {
    async fn transfer(
        &mut self,
        from: Address,
        to: Address,
        amount: u64,
    ) -> io::Result<Bytes32> {
        let script = Opcode::RET(0x10).to_bytes().to_vec();
        let tx = Transaction::script(
            0,
            1_000_000,
            0,
            script,
            vec![],
            vec![Input::CoinSigned {
                utxo_id: self.rng.gen(),
                owner: from,
                amount,
                asset_id: Default::default(),
                tx_pointer: Default::default(),
                witness_index: 0,
                maturity: 0,
            }],
            vec![Output::Coin {
                amount,
                to,
                asset_id: Default::default(),
            }],
            vec![vec![].into()],
        )
        .into();
        self.client.submit_and_await_commit(&tx).await?;
        Ok(tx.id())
    }
}

fn get_executor_and_db() -> (Executor, Database) {
    let db = Database::default();
    let executor = Executor {
        database: db.clone(),
        config: Config::local_node(),
    };

    (executor, db)
}

async fn initialize_client(db: Database) -> FuelClient {
    let config = Config::local_node();
    let service = FuelService::from_database(db, config).await.unwrap();
    FuelClient::from(service.bound_address)
}

// add random val for unique tx
fn create_mock_tx(val: u64) -> Transaction {
    fuel_tx::Transaction::script(
        0,
        0,
        0,
        Default::default(),
        val.to_be_bytes().to_vec(),
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .into()
}
