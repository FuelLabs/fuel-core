use fuel_client::client::FuelClient;
use fuel_core::{
    database::Database,
    model::coin::{Coin, CoinStatus, TxoPointer},
    service::{configure, run_in_background},
};
use fuel_storage::Storage;
use fuel_vm::prelude::{Address, Bytes32, Word};

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
    let mut db = Database::default();
    Storage::<Bytes32, Coin>::insert(&mut db, &id, &coin).unwrap();

    // setup server & client
    let srv = run_in_background(configure(db)).await;
    let client = FuelClient::from(srv);

    // run test
    let coin = client.coin(format!("0x{:X}", id).as_str()).await.unwrap();
    assert!(coin.is_some());
}

#[tokio::test]
async fn first_5_coins() {
    let owner = Address::default();

    // setup test data in the node
    let coins: Vec<(Bytes32, Coin)> = (1..10usize)
        .map(|i| {
            let coin = Coin {
                owner,
                amount: i as Word,
                color: Default::default(),
                maturity: Default::default(),
                status: CoinStatus::Unspent,
                block_created: Default::default(),
            };

            let txo_pointer = TxoPointer {
                block_height: i as u32,
                tx_index: 0,
                output_index: 0,
            };
            (txo_pointer.into(), coin)
        })
        .collect();

    let mut db = Database::default();
    for (id, coin) in coins {
        Storage::<Bytes32, Coin>::insert(&mut db, &id, &coin).unwrap();
    }

    // setup server & client
    let srv = run_in_background(configure(db)).await;
    let client = FuelClient::from(srv);

    // run test
    let coins = client
        .coins_by_owner(format!("0x{:X}", owner).as_str(), Some(5), None, None, None)
        .await
        .unwrap();
    assert!(coins.edges.is_some());
    assert_eq!(coins.edges.unwrap().len(), 5)
}
