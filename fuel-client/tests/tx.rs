use chrono::Utc;
use fuel_core::executor::ExecutionMode;
use fuel_core::model::fuel_block::{FuelBlockFull, FuelBlockHeaders};
use fuel_core::{
    database::Database,
    executor::Executor,
    service::{Config, FuelService},
};
use fuel_gql_client::client::types::TransactionStatus;
use fuel_gql_client::client::{FuelClient, PageDirection, PaginationRequest};
use fuel_vm::{consts::*, prelude::*};
use itertools::Itertools;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::io;

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
async fn dry_run() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let gas_price = 0;
    let gas_limit = 1_000_000;
    let byte_price = 0;
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
        byte_price,
        maturity,
        script,
        vec![],
        vec![],
        vec![],
        vec![],
    );

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
}

#[tokio::test]
async fn submit() {
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let gas_price = 0;
    let gas_limit = 1_000_000;
    let maturity = 0;
    let byte_price = 0;

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
        byte_price,
        maturity,
        script,
        vec![],
        vec![],
        vec![],
        vec![],
    );

    let id = client.submit(&tx).await.unwrap();
    // verify that the tx returned from the api matches the submitted tx
    let ret_tx = client
        .transaction(&id.0.to_string())
        .await
        .unwrap()
        .unwrap()
        .transaction;
    assert_eq!(tx.id(), ret_tx.id());
}

#[tokio::test]
async fn receipts() {
    let transaction = fuel_tx::Transaction::default();
    let id = transaction.id();
    // setup server & client
    let srv = FuelService::new_node(Config::local_node()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    // submit tx
    let result = client.submit(&transaction).await;
    assert!(result.is_ok());

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
    client.submit(&transaction).await.unwrap();

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
    let result = client.submit(&transaction).await;
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

    // there are six transactions
    // [1, 2, 3, 4, 5, 6]

    // Query for first 3: [1,2,3]
    let client = context.client;
    let page_request = PaginationRequest {
        cursor: None,
        results: 3,
        direction: PageDirection::Forward,
    };

    let response = client.transactions(page_request.clone()).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();
    assert_eq!(transactions, &[tx1, tx2, tx3]);

    // Query backwards from last given cursor [3]: [1,2]
    let page_request_backwards = PaginationRequest {
        cursor: response.cursor.clone(),
        results: 3,
        direction: PageDirection::Backward,
    };

    // Query forwards from last given cursor [3]: [4,5,6]
    let page_request_forwards = PaginationRequest {
        cursor: response.cursor,
        results: 3,
        direction: PageDirection::Forward,
    };

    let response = client.transactions(page_request_backwards).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();
    assert_eq!(transactions, &[tx1, tx2]);

    let response = client.transactions(page_request_forwards).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();
    assert_eq!(transactions, &[tx4, tx5, tx6]);
}

#[tokio::test]
async fn get_transactions_from_manual_blocks() {
    let (executor, db) = get_executor_and_db();
    // get access to a client
    let client = initialize_client(db).await;

    // create 10 txs
    let txs: Vec<Transaction> = (0..10).map(create_mock_tx).collect();

    // make 1st test block
    let mut first_test_block = FuelBlockFull {
        headers: FuelBlockHeaders {
            fuel_height: 1u32.into(),
            time: Utc::now(),
            producer: Default::default(),
            transactions_commitment: Default::default(),
        },

        // set the first 5 ids of the manually saved txs
        transactions: txs.iter().take(5).cloned().collect(),
    };

    // make 2nd test block
    let mut second_test_block = FuelBlockFull {
        headers: FuelBlockHeaders {
            fuel_height: 2u32.into(),
            time: Utc::now(),
            producer: Default::default(),
            transactions_commitment: Default::default(),
        },
        // set the last 5 ids of the manually saved txs
        transactions: txs.iter().skip(5).take(5).cloned().collect(),
    };

    // process blocks and save block height
    executor
        .execute(&mut first_test_block, ExecutionMode::Production)
        .await
        .unwrap();
    executor
        .execute(&mut second_test_block, ExecutionMode::Production)
        .await
        .unwrap();

    // Query for first 3: [0,1,2]
    let page_request_forwards = PaginationRequest {
        cursor: None,
        results: 3,
        direction: PageDirection::Forward,
    };
    let response = client.transactions(page_request_forwards).await.unwrap();
    let transactions = &response
        .results
        .iter()
        .map(|tx| tx.transaction.id())
        .collect_vec();
    assert_eq!(transactions, &[txs[0].id(), txs[1].id(), txs[2].id()]);

    // Query forwards from last given cursor [2]: [3,4,5,6]
    let next_page_request_forwards = PaginationRequest {
        cursor: response.cursor,
        results: 4,
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
    assert_eq!(
        transactions,
        &[txs[3].id(), txs[4].id(), txs[5].id(), txs[6].id()]
    );

    // Query backwards from last given cursor [6]: [0,1,2,3,4,5]
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
    assert_eq!(
        transactions,
        &[
            txs[0].id(),
            txs[1].id(),
            txs[2].id(),
            txs[3].id(),
            txs[4].id(),
            txs[5].id()
        ]
    );
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

struct TestContext {
    rng: StdRng,
    pub client: FuelClient,
}

impl TestContext {
    async fn new(seed: u64) -> Self {
        let rng = StdRng::seed_from_u64(seed);
        let srv = FuelService::new_node(Config::local_node()).await.unwrap();
        let client = FuelClient::from(srv.bound_address);
        Self { rng, client }
    }

    async fn transfer(&mut self, from: Address, to: Address, amount: u64) -> io::Result<Bytes32> {
        let script = Opcode::RET(0x10).to_bytes().to_vec();
        let tx = Transaction::Script {
            gas_price: 0,
            gas_limit: 1_000_000,
            byte_price: 0,
            maturity: 0,
            receipts_root: Default::default(),
            script,
            script_data: vec![],
            inputs: vec![Input::Coin {
                utxo_id: self.rng.gen(),
                owner: from,
                amount,
                color: Default::default(),
                witness_index: 0,
                maturity: 0,
                predicate: vec![],
                predicate_data: vec![],
            }],
            outputs: vec![Output::Coin {
                amount,
                to,
                color: Default::default(),
            }],
            witnesses: vec![vec![].into()],
            metadata: None,
        };
        self.client.submit(&tx).await.map(Into::into)
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
        0,
        Default::default(),
        val.to_be_bytes().to_vec(),
        Default::default(),
        Default::default(),
        Default::default(),
    )
}
