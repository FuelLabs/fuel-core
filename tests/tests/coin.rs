use fuel_core::{
    chain_config::{
        CoinConfig,
        CoinConfigGenerator,
        StateConfig,
    },
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    types::primitives::{
        Address,
        AssetId,
        UtxoId,
    },
    FuelClient,
};
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::TxId,
};
use rstest::rstest;

async fn setup_service(configs: Vec<CoinConfig>) -> FuelService {
    let state = StateConfig {
        coins: configs,
        ..Default::default()
    };
    let config = Config::local_node_with_state_config(state);

    FuelService::from_database(Database::default(), config)
        .await
        .unwrap()
}

#[tokio::test]
async fn coin() {
    // setup test data in the node
    let output_index = 5;
    let tx_id = TxId::new([1u8; 32]);
    let coin = CoinConfig {
        output_index,
        tx_id,
        ..Default::default()
    };

    // setup server & client
    let srv = setup_service(vec![coin]).await;
    let client = FuelClient::from(srv.bound_address);

    // run test
    let utxo_id = UtxoId::new(tx_id, output_index);
    let coin = client.coin(&utxo_id).await.unwrap();
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
    let mut coin_generator = CoinConfigGenerator::new();
    let coins: Vec<_> = (1..10usize)
        .map(|i| CoinConfig {
            owner,
            amount: i as Word,
            ..coin_generator.generate()
        })
        .collect();

    // setup server & client
    let srv = setup_service(coins).await;
    let client = FuelClient::from(srv.bound_address);

    // run test
    let coins = client
        .coins(
            &owner,
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
    let mut coin_generator = CoinConfigGenerator::new();
    let coins: Vec<_> = (1..10usize)
        .map(|i| CoinConfig {
            owner,
            amount: i as Word,
            asset_id: if i <= 5 { asset_id } else { Default::default() },
            ..coin_generator.generate()
        })
        .collect();

    // setup server & client
    let srv = setup_service(coins).await;
    let client = FuelClient::from(srv.bound_address);

    // run test
    let coins = client
        .coins(
            &owner,
            Some(&asset_id),
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
    assert!(coins.results.into_iter().all(|c| asset_id == c.asset_id));
}

#[rstest]
#[tokio::test]
async fn get_coins_forwards_backwards(
    #[values(Address::default(), Address::from([16; 32]))] owner: Address,
    #[values(AssetId::from([1u8; 32]), AssetId::from([32u8; 32]))] asset_id: AssetId,
) {
    // setup test data in the node
    let coins: Vec<_> = (1..11usize)
        .map(|i| CoinConfig {
            owner,
            amount: i as Word,
            asset_id,
            output_index: i as u16,
            ..Default::default()
        })
        .collect();

    // setup server & client
    let srv = setup_service(coins).await;
    let client = FuelClient::from(srv.bound_address);

    // run test
    let coins = client
        .coins(
            &owner,
            Some(&asset_id),
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
