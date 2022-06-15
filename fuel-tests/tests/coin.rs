use fuel_core::{
    chain_config::{CoinConfig, StateConfig},
    database::Database,
    model::{Coin, CoinStatus},
    service::{Config, FuelService},
};
use fuel_core_interfaces::common::{
    fuel_storage::Storage,
    fuel_tx::{AssetId, UtxoId},
    fuel_vm::prelude::{Address, Bytes32, Word},
};
use fuel_gql_client::client::{
    schema::coin::CoinStatus as SchemeCoinStatus, FuelClient, PageDirection, PaginationRequest,
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
        .all(|c| c.status == SchemeCoinStatus::Unspent));
}

#[tokio::test]
async fn coins_to_spend() {
    let owner = Address::default();
    let asset_id_a = AssetId::new([1u8; 32]);
    let asset_id_b = AssetId::new([2u8; 32]);

    // setup config
    let mut config = Config::local_node();
    config.chain_conf.initial_state = Some(StateConfig {
        height: None,
        contracts: None,
        coins: Some(
            vec![
                (owner, 50, asset_id_a),
                (owner, 100, asset_id_a),
                (owner, 150, asset_id_a),
                (owner, 50, asset_id_b),
                (owner, 100, asset_id_b),
                (owner, 150, asset_id_b),
            ]
            .into_iter()
            .map(|(owner, amount, asset_id)| CoinConfig {
                tx_id: None,
                output_index: None,
                block_created: None,
                maturity: None,
                owner,
                amount,
                asset_id,
            })
            .collect(),
        ),
    });

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // empty spend_query
    let coins = client
        .coins_to_spend(format!("{:#x}", owner).as_str(), vec![], None, None)
        .await
        .unwrap();
    assert!(coins.is_empty());

    // spend_query for 1 a and 1 b
    let coins = client
        .coins_to_spend(
            format!("{:#x}", owner).as_str(),
            vec![
                (format!("{:#x}", asset_id_a).as_str(), 1),
                (format!("{:#x}", asset_id_b).as_str(), 1),
            ],
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(coins.len(), 2);

    // spend_query for 300 a and 300 b
    let coins = client
        .coins_to_spend(
            format!("{:#x}", owner).as_str(),
            vec![
                (format!("{:#x}", asset_id_a).as_str(), 300),
                (format!("{:#x}", asset_id_b).as_str(), 300),
            ],
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(coins.len(), 6);

    // spend_query for 1 a and 1 b, but with all coins excluded
    let all_coin_ids = coins
        .iter()
        .map(|c| format!("{:#x}", c.utxo_id))
        .collect::<Vec<String>>();
    let all_coin_ids = all_coin_ids.iter().map(String::as_str).collect();
    let coins = client
        .coins_to_spend(
            format!("{:#x}", owner).as_str(),
            vec![
                (format!("{:#x}", asset_id_a).as_str(), 1),
                (format!("{:#x}", asset_id_b).as_str(), 1),
            ],
            None,
            Some(all_coin_ids),
        )
        .await;
    assert!(coins.is_err());

    // not enough coins
    let coins = client
        .coins_to_spend(
            format!("{:#x}", owner).as_str(),
            vec![
                (format!("{:#x}", asset_id_a).as_str(), 301),
                (format!("{:#x}", asset_id_b).as_str(), 301),
            ],
            None,
            None,
        )
        .await;
    assert!(coins.is_err());

    // not enough inputs
    let coins = client
        .coins_to_spend(
            format!("{:#x}", owner).as_str(),
            vec![
                (format!("{:#x}", asset_id_a).as_str(), 300),
                (format!("{:#x}", asset_id_b).as_str(), 300),
            ],
            5.into(),
            None,
        )
        .await;
    assert!(coins.is_err());
}
