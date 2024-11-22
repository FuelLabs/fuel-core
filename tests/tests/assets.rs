use fuel_core::{
    chain_config::{
        CoinConfig,
        StateConfig,
    },
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::{
        primitives::AssetId,
        TransactionStatus,
    },
    FuelClient,
};
use fuel_core_types::fuel_tx::{
    Input,
    Output,
    TransactionBuilder,
    TxId,
};

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
#[ignore] // TODO: Need to be able to mint assets for this test to work
async fn asset_info() {
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
    let asset_id = AssetId::new([1u8; 32]);
    let mint_amount = 100;
    let burn_amount = 50;

    // Mint coins first
    let mint_tx = TransactionBuilder::mint(
        0u32.into(),
        0,
        Default::default(),
        Default::default(),
        mint_amount,
        asset_id,
        Default::default(),
    )
    .finalize_as_transaction();

    let status = client.submit_and_await_commit(&mint_tx).await.unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // Query asset info before burn
    let initial_supply = client
        .asset_info(&asset_id)
        .await
        .unwrap()
        .unwrap()
        .total_supply
        .0;

    // We should have the minted amount first
    assert_eq!(initial_supply, mint_amount);

    // Create and submit transaction that burns coins
    let tx = TransactionBuilder::script(vec![], vec![])
        .add_input(Input::coin_signed(
            Default::default(),
            Default::default(),
            burn_amount,
            asset_id,
            Default::default(),
            0,
        ))
        .add_output(Output::Change {
            to: Default::default(),
            amount: 0,
            asset_id,
        })
        .finalize_as_transaction();

    let status = client.submit_and_await_commit(&tx).await.unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // Query asset info after burn
    let final_supply = client
        .asset_info(&asset_id)
        .await
        .unwrap()
        .unwrap()
        .total_supply
        .0;

    // Verify burn was recorded
    assert_eq!(final_supply, initial_supply - burn_amount);
}
