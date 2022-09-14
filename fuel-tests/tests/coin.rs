use fuel_core::{
    database::Database,
    model::{
        Coin,
        CoinStatus,
    },
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::StorageAsMut,
        fuel_tx::{
            AssetId,
            UtxoId,
        },
        fuel_vm::prelude::{
            Address,
            Bytes32,
            Word,
        },
    },
    db::Coins,
};
use fuel_gql_client::client::{
    schema::coin::CoinStatus as SchemeCoinStatus,
    FuelClient,
    PageDirection,
    PaginationRequest,
};

#[tokio::test]
async fn coin() {
    // setup test data in the node
    let coin = Coin {
        owner: Default::default(),
        amount: 0,
        asset_id: Default::default(),
        maturity: Default::default(),
        status: CoinStatus::Unspent,
        block_created: Default::default(),
    };

    let utxo_id = UtxoId::new(Default::default(), 5);

    let mut db = Database::default();
    db.storage::<Coins>().insert(&utxo_id, &coin).unwrap();
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
                asset_id: Default::default(),
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
        db.storage::<Coins>().insert(&utxo_id, &coin).unwrap();
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
async fn only_asset_id_filtered_coins() {
    let owner = Address::default();
    let asset_id = AssetId::new([1u8; 32]);

    // setup test data in the node
    let coins: Vec<(UtxoId, Coin)> = (1..10usize)
        .map(|i| {
            let coin = Coin {
                owner,
                amount: i as Word,
                asset_id: if i <= 5 { asset_id } else { Default::default() },
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
        db.storage::<Coins>().insert(&id, &coin).unwrap();
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
            Some(format!("{:#x}", AssetId::new([1u8; 32])).as_str()),
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
        .all(|c| asset_id == c.asset_id.into()));
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
                asset_id: Default::default(),
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
        db.storage::<Coins>().insert(&id, &coin).unwrap();
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
        .all(|c| c.status == SchemeCoinStatus::Unspent));
}
