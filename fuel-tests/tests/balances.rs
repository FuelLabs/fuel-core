use fuel_core::{
    config::{
        chain_config::{CoinConfig, StateConfig},
        Config,
    },
    service::FuelService,
};
use fuel_core_interfaces::common::{
    fuel_tx::{AssetId, Input, Output},
    fuel_vm::prelude::Address,
};
use fuel_gql_client::{
    client::{FuelClient, PageDirection, PaginationRequest},
    fuel_tx::TransactionBuilder,
};

#[tokio::test]
async fn balance() {
    let owner = Address::default();
    let asset_id = AssetId::new([1u8; 32]);

    // setup config
    let mut config = Config::local_node();
    config.chain_conf.initial_state = Some(StateConfig {
        height: None,
        contracts: None,
        coins: Some(
            vec![
                (owner, 50, asset_id),
                (owner, 100, asset_id),
                (owner, 150, asset_id),
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

    // run test
    let balance = client
        .balance(
            format!("{:#x}", owner).as_str(),
            Some(format!("{:#x}", asset_id).as_str()),
        )
        .await
        .unwrap();
    assert_eq!(balance, 300);

    // spend some coins and check again
    let coins = client
        .coins_to_spend(
            format!("{:#x}", owner).as_str(),
            vec![(format!("{:#x}", asset_id).as_str(), 1)],
            None,
            None,
        )
        .await
        .unwrap();

    let mut tx = TransactionBuilder::script(vec![], vec![])
        .gas_limit(1_000_000)
        .to_owned();
    for coin in coins {
        tx.add_input(Input::CoinSigned {
            utxo_id: coin.utxo_id.into(),
            owner: coin.owner.into(),
            amount: coin.amount.into(),
            asset_id: coin.asset_id.into(),
            maturity: coin.maturity.into(),
            witness_index: 0,
        });
    }
    let tx = tx
        .add_output(Output::Coin {
            to: Address::new([1u8; 32]),
            amount: 1,
            asset_id,
        })
        .add_output(Output::Change {
            to: owner,
            amount: 0,
            asset_id,
        })
        .add_witness(Default::default())
        .finalize();

    client.submit(&tx).await.unwrap();

    let balance = client
        .balance(
            format!("{:#x}", owner).as_str(),
            Some(format!("{:#x}", asset_id).as_str()),
        )
        .await
        .unwrap();
    assert_eq!(balance, 299);
}

#[tokio::test]
async fn first_5_balances() {
    let owner = Address::default();
    let asset_ids = (1..=6u8)
        .map(|i| AssetId::new([i; 32]))
        .collect::<Vec<AssetId>>();

    // setup config
    let mut config = Config::local_node();
    config.chain_conf.initial_state = Some(StateConfig {
        height: None,
        contracts: None,
        coins: Some(
            asset_ids
                .clone()
                .into_iter()
                .flat_map(|asset_id| {
                    vec![
                        (owner, 50, asset_id),
                        (owner, 100, asset_id),
                        (owner, 150, asset_id),
                    ]
                })
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

    // run test
    let balances = client
        .balances(
            format!("{:#x}", owner).as_str(),
            PaginationRequest {
                cursor: None,
                results: 5,
                direction: PageDirection::Forward,
            },
        )
        .await
        .unwrap();
    assert!(!balances.results.is_empty());
    assert_eq!(balances.results.len(), 5);
    assert_eq!(balances.results[0].asset_id.0 .0, asset_ids[0]);
    assert_eq!(balances.results[0].amount.0, 300);
}
