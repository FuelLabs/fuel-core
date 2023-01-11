use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    FuelClient,
    PageDirection,
    PaginationRequest,
};
use fuel_core_storage::{
    tables::Coins,
    StorageAsMut,
};
use fuel_core_types::{
    entities::coin::{
        Coin,
        CoinStatus,
    },
    fuel_asm::*,
    fuel_tx::*,
};
use rstest::rstest;

#[tokio::test]
async fn coin() {
    // setup test data in the node
    let utxo_id = UtxoId::new(Default::default(), 5);

    // setup server & client
    let srv = FuelService::from_database(Database::default(), Config::local_node())
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

// Backward fails, tracking in https://github.com/FuelLabs/fuel-core/issues/610
#[rstest]
#[tokio::test]
async fn first_5_coins(
    #[values(PageDirection::Forward)] pagination_direction: PageDirection,
) {
    let owner = Address::default();

    // setup test data in the node
    let coins: Vec<_> = (1..10usize)
        .map(|i| Coin {
            utxo_id: UtxoId::new(Bytes32::from([i as u8; 32]), 0),
            owner,
            amount: i as Word,
            asset_id: Default::default(),
            maturity: Default::default(),
            status: fuel_core_types::entities::coin::CoinStatus::Unspent,
            block_created: Default::default(),
        })
        .collect();

    let mut db = Database::default();
    for coin in coins {
        db.storage::<Coins>()
            .insert(&coin.utxo_id.clone(), &coin.compress())
            .unwrap();
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
                direction: pagination_direction,
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
    let coins: Vec<_> = (1..10usize)
        .map(|i| Coin {
            utxo_id: UtxoId::new(Bytes32::from([i as u8; 32]), 0),
            owner,
            amount: i as Word,
            asset_id: if i <= 5 { asset_id } else { Default::default() },
            maturity: Default::default(),
            status: CoinStatus::Unspent,
            block_created: Default::default(),
        })
        .collect();

    let mut db = Database::default();
    for coin in coins {
        db.storage::<Coins>()
            .insert(&coin.utxo_id.clone(), &coin.compress())
            .unwrap();
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
            Some(format!("{:#x}", asset_id).as_str()),
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

#[rstest]
#[tokio::test]
async fn get_unspent_and_spent_coins(
    #[values(Address::default(), Address::from([16; 32]))] owner: Address,
    #[values(AssetId::from([1u8; 32]), AssetId::from([32u8; 32]))] asset_id: AssetId,
) {
    // setup test data in the node
    let coins: Vec<Coin> = (1..11usize)
        .map(|i| Coin {
            utxo_id: UtxoId::new(Bytes32::from([i as u8; 32]), 0),
            owner,
            amount: i as Word,
            asset_id,
            maturity: Default::default(),
            status: if i <= 5 {
                CoinStatus::Unspent
            } else {
                CoinStatus::Spent
            },
            block_created: Default::default(),
        })
        .collect();

    let mut db = Database::default();
    for coin in coins {
        db.storage::<Coins>()
            .insert(&coin.utxo_id.clone(), &coin.compress())
            .unwrap();
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
            Some(format!("{:#x}", asset_id).as_str()),
            PaginationRequest {
                cursor: None,
                results: 10,
                direction: PageDirection::Forward,
            },
        )
        .await
        .unwrap();
    assert!(!coins.results.is_empty());
    assert_eq!(coins.results.len(), 10);
}
