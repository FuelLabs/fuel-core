use fuel_core::{
    chain_config::{
        CoinConfig,
        MessageConfig,
        StateConfig,
    },
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    schema::resource::Resource,
    FuelClient,
    PageDirection,
    PaginationRequest,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_tx::*,
    fuel_types::Address,
};

#[tokio::test]
async fn balance() {
    let owner = Address::default();
    let asset_id = AssetId::BASE;

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
        messages: Some(
            vec![(owner, 60), (owner, 90)]
                .into_iter()
                .enumerate()
                .map(|(nonce, (owner, amount))| MessageConfig {
                    sender: owner,
                    recipient: owner,
                    nonce: nonce as u64,
                    amount,
                    data: vec![],
                    da_height: DaBlockHeight::from(1usize),
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
            format!("{owner:#x}").as_str(),
            Some(format!("{asset_id:#x}").as_str()),
        )
        .await
        .unwrap();
    assert_eq!(balance, 450);

    // spend some resources and check again
    let resources_per_asset = client
        .resources_to_spend(
            format!("{owner:#x}").as_str(),
            vec![(format!("{asset_id:#x}").as_str(), 1, None)],
            None,
        )
        .await
        .unwrap();

    let mut tx = TransactionBuilder::script(vec![], vec![])
        .gas_limit(1_000_000)
        .to_owned();
    for resources in resources_per_asset {
        for resource in resources {
            match resource {
                Resource::Coin(coin) => tx.add_input(Input::CoinSigned {
                    utxo_id: coin.utxo_id.into(),
                    owner: coin.owner.into(),
                    amount: coin.amount.into(),
                    asset_id: coin.asset_id.into(),
                    maturity: coin.maturity.into(),
                    witness_index: 0,
                    tx_pointer: Default::default(),
                }),
                Resource::Message(message) => tx.add_input(Input::MessageSigned {
                    message_id: message.message_id.into(),
                    sender: message.sender.into(),
                    amount: message.amount.into(),
                    witness_index: 0,
                    recipient: message.recipient.into(),
                    nonce: message.nonce.into(),
                    data: Default::default(),
                }),
                Resource::Unknown => panic!("Unknown resource"),
            };
        }
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
        .finalize_as_transaction();

    client.submit_and_await_commit(&tx).await.unwrap();

    let balance = client
        .balance(
            format!("{owner:#x}").as_str(),
            Some(format!("{asset_id:#x}").as_str()),
        )
        .await
        .unwrap();
    assert_eq!(balance, 449);
}

#[tokio::test]
async fn first_5_balances() {
    let owner = Address::from([10u8; 32]);
    let asset_ids = (0..=5u8)
        .map(|i| AssetId::new([i; 32]))
        .collect::<Vec<AssetId>>();

    let all_owners = vec![Address::default(), owner, Address::from([20u8; 32])];
    let coins = {
        // setup all coins for all owners
        let mut coins = vec![];
        for owner in &all_owners {
            coins.extend(
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
                        owner: *owner,
                        amount,
                        asset_id,
                    }),
            );
        }
        coins
    };

    let messages = {
        // setup all messages for all owners
        let mut messages = vec![];
        for owner in &all_owners {
            messages.extend(vec![(owner, 60), (owner, 90)].into_iter().enumerate().map(
                |(nonce, (owner, amount))| MessageConfig {
                    sender: *owner,
                    recipient: *owner,
                    nonce: nonce as u64,
                    amount,
                    data: vec![],
                    da_height: DaBlockHeight::from(1usize),
                },
            ))
        }
        messages
    };

    // setup config
    let mut config = Config::local_node();
    config.chain_conf.initial_state = Some(StateConfig {
        height: None,
        contracts: None,
        coins: Some(coins),
        messages: Some(messages),
    });

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    let balances = client
        .balances(
            format!("{owner:#x}").as_str(),
            PaginationRequest {
                cursor: None,
                results: 5,
                direction: PageDirection::Forward,
            },
        )
        .await
        .unwrap();
    let balances = balances.results;
    assert!(!balances.is_empty());
    assert_eq!(balances.len(), 5);

    // Base asset is 3 coins and 2 messages = 50 + 100 + 150 + 60 + 90
    assert_eq!(balances[0].asset_id.0 .0, asset_ids[0]);
    assert_eq!(balances[0].amount.0, 450);

    // Other assets are 3 coins = 50 + 100 + 150
    for i in 1..5 {
        assert_eq!(balances[i].asset_id.0 .0, asset_ids[i]);
        assert_eq!(balances[i].amount.0, 300);
    }
}
