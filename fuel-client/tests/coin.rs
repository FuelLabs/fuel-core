use fuel_client::client::FuelClient;
use fuel_core::database::{KvStore, SharedDatabase};
use fuel_core::model::coin::{Coin, CoinStatus, TxoPointer};
use fuel_core::schema::scalars::HexString256;
use fuel_core::service::{configure, run_in_background};
use fuel_vm::prelude::Bytes32;

#[tokio::test]
async fn coin() {
    // setup test data in the node
    let coin = Coin {
        owner: Default::default(),
        amount: 0,
        color: Default::default(),
        maturity: Default::default(),
        status: CoinStatus::Unspent,
        block_created: Default::default(),
    };

    let txo_pointer = TxoPointer {
        block_height: 0,
        tx_index: 0,
        output_index: 0,
    };

    let id: Bytes32 = txo_pointer.into();
    let db = SharedDatabase::default();
    KvStore::<Bytes32, Coin>::insert(db.as_ref(), &id, &coin).unwrap();

    // setup server & client
    let srv = run_in_background(configure(db)).await;
    let client = FuelClient::from(srv);

    // run test
    let coin = client
        .coin(HexString256::from(id).to_string().as_str())
        .await
        .unwrap();
    assert!(coin.is_some());
}
