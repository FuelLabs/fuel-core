use fuel_core::{
    database::Database,
    model::coin::{Coin, CoinStatus},
    service::{Config, FuelService},
};
use fuel_gql_client::client::{
    schema::coin::CoinStatus as SchemaCoinStatus, FuelClient, PageDirection, PaginationRequest,
};
use fuel_storage::Storage;
use fuel_tx::{Color, UtxoId};
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

    let utxo_id = UtxoId::new(Default::default(), 5);

    let mut db = Database::default();
    Storage::<UtxoId, Coin>::insert(&mut db, &utxo_id, &coin).unwrap();
    // setup server & client
    let srv = FuelService::from_database(db, Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    let coin = client
        .coin(format!("{:#x}", utxo_id).as_str())
        .await
        .unwrap();
    assert!(coin.is_some());
}

#[tokio::test]
async fn first_5_coins() {
    let owner = Address::default();

    // setup test data in the node
    let coins: Vec<(UtxoId, Coin)> = (1..10usize)
        .map(|i| {
            let coin = Coin {
                owner,
                amount: i as Word,
                color: Default::default(),
                maturity: Default::default(),
                status: CoinStatus::Unspent,
                block_created: Default::default(),
            };

            let utxo_id = UtxoId::new(Bytes32::from([i as u8; 32]), 0);
            (utxo_id, coin)
        })
        .collect();

    let mut db = Database::default();
    for (utxo_id, coin) in coins {
        Storage::<UtxoId, Coin>::insert(&mut db, &utxo_id, &coin).unwrap();
    }

    // setup server & client
    let srv = FuelService::from_database(db, Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    let coins = client
        .coins(
            format!("{:#x}", owner).as_str(),
            None,
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

#[tokio::test]
async fn only_color_filtered_coins() {
    let owner = Address::default();
    let color = Color::new([1u8; 32]);

    // setup test data in the node
    let coins: Vec<(UtxoId, Coin)> = (1..10usize)
        .map(|i| {
            let coin = Coin {
                owner,
                amount: i as Word,
                color: if i <= 5 { color } else { Default::default() },
                maturity: Default::default(),
                status: CoinStatus::Unspent,
                block_created: Default::default(),
            };

            let utxo_id = UtxoId::new(Bytes32::from([i as u8; 32]), 0);
            (utxo_id, coin)
        })
        .collect();

    let mut db = Database::default();
    for (id, coin) in coins {
        Storage::<UtxoId, Coin>::insert(&mut db, &id, &coin).unwrap();
    }

    // setup server & client
    let srv = FuelService::from_database(db, Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    let coins = client
        .coins(
            format!("{:#x}", owner).as_str(),
            Some(format!("{:#x}", Color::new([1u8; 32])).as_str()),
            PaginationRequest {
                cursor: None,
                results: 10,
                direction: PageDirection::Forward,
            },
        )
        .await
        .unwrap();
    assert!(!coins.results.is_empty());
    assert_eq!(coins.results.len(), 5);
    assert!(coins.results.into_iter().all(|c| color == c.color.into()));
}

#[tokio::test]
async fn only_unspent_coins() {
    let owner = Address::default();

    // setup test data in the node
    let coins: Vec<(UtxoId, Coin)> = (1..10usize)
        .map(|i| {
            let coin = Coin {
                owner,
                amount: i as Word,
                color: Default::default(),
                maturity: Default::default(),
                status: if i <= 5 {
                    CoinStatus::Unspent
                } else {
                    CoinStatus::Spent
                },
                block_created: Default::default(),
            };

            let utxo_id = UtxoId::new(Bytes32::from([i as u8; 32]), 0);
            (utxo_id, coin)
        })
        .collect();

    let mut db = Database::default();
    for (id, coin) in coins {
        Storage::<UtxoId, Coin>::insert(&mut db, &id, &coin).unwrap();
    }

    // setup server & client
    let srv = FuelService::from_database(db, Config::local_node())
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    let coins = client
        .coins(
            format!("{:#x}", owner).as_str(),
            None,
            PaginationRequest {
                cursor: None,
                results: 10,
                direction: PageDirection::Forward,
            },
        )
        .await
        .unwrap();
    assert!(!coins.results.is_empty());
    assert_eq!(coins.results.len(), 5);
    assert!(coins
        .results
        .into_iter()
        .all(|c| c.status == SchemaCoinStatus::Unspent));
}
