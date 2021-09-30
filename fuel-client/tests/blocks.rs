use fuel_client::client::FuelClient;
use fuel_core::database::{KvStore, SharedDatabase};
use fuel_core::model::fuel_block::FuelBlock;
use fuel_core::schema::scalars::HexString256;
use fuel_core::service::{configure, run_in_background};
use fuel_vm::prelude::Bytes32;

#[tokio::test]
async fn blocks() {
    // setup test data in the node
    let block = FuelBlock::default();
    let id = block.id();
    let db = SharedDatabase::default();
    KvStore::<Bytes32, FuelBlock>::insert(db.as_ref(), &id, &block).unwrap();

    // setup server & client
    let srv = run_in_background(configure(db)).await;
    let client = FuelClient::from(srv);

    // run test
    let block = client
        .block(HexString256::from(id).to_string().as_str())
        .await
        .unwrap();
    assert!(block.is_some());
}
