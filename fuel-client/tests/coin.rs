use fuel_client::client::{FuelClient, PageDirection, PaginationRequest};
use fuel_core::{
    database::Database,
    model::coin::{Coin, CoinStatus, UtxoId},
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

    let utxo_id = UtxoId {
        tx_id: Default::default(),
        output_index: 0,
    };

    let id: Bytes32 = utxo_id.into();
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

            let utxo_id = UtxoId {
                tx_id: Bytes32::from([i as u8; 32]),
                output_index: 0,
            };
            (utxo_id.into(), coin)
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
        .coins_by_owner(
            format!("0x{:X}", owner).as_str(),
            PaginationRequest {
                cursor: None,
                results: 5,
                direction: PageDirection::Forward,
            },
        )
        .await
        .unwrap();
    assert!(!coins.results.is_empty());
    assert_eq!(coins.results.len(), 5)
}
