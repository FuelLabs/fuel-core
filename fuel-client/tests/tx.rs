use fuel_client::client::FuelClient;
use fuel_core::database::{KvStore, SharedDatabase};
use fuel_core::schema::scalars::HexString256;
use fuel_core::service::{configure, run_in_background};
use fuel_vm::consts::*;
use fuel_vm::prelude::*;

#[tokio::test]
async fn transact() {
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
async fn get_transaction_by_id() {
    // setup test data in the node
    let transaction = fuel_tx::Transaction::default();
    let id = transaction.id();
    let db = SharedDatabase::default();
    KvStore::<Bytes32, fuel_tx::Transaction>::insert(db.as_ref(), &id, &transaction).unwrap();

    // setup server & client
    let srv = run_in_background(configure(db)).await;
    let client = FuelClient::from(srv);

    // run test
    let transaction = client
        .transaction(&HexString256::from(id).to_string())
        .await
        .unwrap();
    assert!(transaction.is_some());
}

#[tokio::test]
async fn get_transparent_transaction_by_id() {
    // setup test data in the node
    let transaction = fuel_tx::Transaction::default();
    let id = transaction.id();

    // setup server & client
    let srv = run_in_background(configure(Default::default())).await;
    let client = FuelClient::from(srv);

    // submit tx
    let result = client.submit(&transaction).await;
    assert!(result.is_ok());

    let opaque_tx = client
        .transaction(&HexString256::from(id.clone()).to_string())
        .await
        .unwrap()
        .expect("expected some result");

    // run test
    let transparent_transaction = client
        .transparent_transaction(&HexString256::from(id).to_string())
        .await
        .unwrap()
        .expect("expected some value");

    // verify transaction round-trips via transparent graphql
    assert_eq!(opaque_tx, transparent_transaction);
}
