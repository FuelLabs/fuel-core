use fuel_client::client::{FuelClient, PageDirection, PaginationRequest};
use fuel_core::database::Database;
use fuel_core::model::coin::UtxoId;
use fuel_core::service::{configure, run_in_background};
use fuel_storage::Storage;
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
        .map(|op| u32::from(*op).to_be_bytes())
        .flatten()
        .collect();
    insta::assert_snapshot!(format!("{:?}", script));
}

#[tokio::test]
async fn dry_run() {
    let srv = run_in_background(configure(Default::default())).await;
    let client = FuelClient::from(srv);

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
        .map(|op| u32::from(*op).to_be_bytes())
        .flatten()
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
    );

    let log = client.dry_run(&tx).await.unwrap();
    assert_eq!(2, log.len());

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
    let srv = run_in_background(configure(Default::default())).await;
    let client = FuelClient::from(srv);

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
        .map(|op| u32::from(*op).to_be_bytes())
        .flatten()
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
    );

    let id = client.submit(&tx).await.unwrap();
    // verify that the tx returned from the api matches the submitted tx
    let ret_tx = client
        .transaction(&id.0.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tx, ret_tx);
}

#[tokio::test]
async fn receipts() {
    let transaction = fuel_tx::Transaction::default();
    let id = transaction.id();
    // setup server & client
    let srv = run_in_background(configure(Default::default())).await;
    let client = FuelClient::from(srv);
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
    let mut db = Database::default();
    Storage::<Bytes32, fuel_tx::Transaction>::insert(&mut db, &id, &transaction).unwrap();

    // setup server & client
    let srv = run_in_background(configure(db)).await;
    let client = FuelClient::from(srv);

    // run test
    let transaction = client.transaction(&format!("{:#x}", id)).await.unwrap();
    assert!(transaction.is_some());
}

#[tokio::test]
async fn get_transparent_transaction_by_id() {
    let transaction = fuel_tx::Transaction::default();
    let id = transaction.id();

    // setup server & client
    let srv = run_in_background(configure(Default::default())).await;
    let client = FuelClient::from(srv);

    // submit tx
    let result = client.submit(&transaction).await;
    assert!(result.is_ok());

    let opaque_tx = client
        .transaction(&format!("{:#x}", id))
        .await
        .unwrap()
        .expect("expected some result");

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
        .map(|tx| tx.id())
        .collect_vec();

    let bob_txs = client
        .transactions_by_owner(&format!("{:#x}", bob), page_request.clone())
        .await
        .unwrap()
        .results
        .iter()
        .map(|tx| tx.id())
        .collect_vec();

    let charlie_txs = client
        .transactions_by_owner(&format!("{:#x}", charlie), page_request.clone())
        .await
        .unwrap()
        .results
        .iter()
        .map(|tx| tx.id())
        .collect_vec();

    assert_eq!(&alice_txs, &[tx1.clone()]);
    assert_eq!(&bob_txs, &[tx2.clone(), tx3.clone()]);
    assert_eq!(&charlie_txs, &[tx1, tx2, tx3]);
}

struct TestContext {
    rng: StdRng,
    pub client: FuelClient,
}

impl TestContext {
    async fn new(seed: u64) -> Self {
        let rng = StdRng::seed_from_u64(seed);
        let server = run_in_background(configure(Default::default())).await;
        let client = FuelClient::from(server);
        Self { rng, client }
    }

    async fn transfer(&mut self, from: Address, to: Address, amount: u64) -> io::Result<Bytes32> {
        let script = Opcode::RET(0x10).to_bytes().to_vec();
        let tx = Transaction::Script {
            gas_price: 0,
            gas_limit: 1_000_000,
            maturity: 0,
            receipts_root: Default::default(),
            script,
            script_data: vec![],
            inputs: vec![Input::Coin {
                utxo_id: UtxoId {
                    tx_id: self.rng.gen(),
                    output_index: 0,
                }
                .into(),
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
