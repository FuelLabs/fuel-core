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
    schema::coins::CoinType,
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
                tx_pointer_block_height: None,
                tx_pointer_tx_idx: None,
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
                    nonce: (nonce as u64).into(),
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

    // spend some coins and check again
    let coins_per_asset = client
        .coins_to_spend(
            format!("{owner:#x}").as_str(),
            vec![(format!("{asset_id:#x}").as_str(), 1, None)],
            None,
        )
        .await
        .unwrap();

    let mut tx = TransactionBuilder::script(vec![], vec![])
        .gas_limit(1_000_000)
        .to_owned();
    for coins in coins_per_asset {
        for coin in coins {
            match coin {
                CoinType::Coin(coin) => tx.add_input(Input::coin_signed(
                    coin.utxo_id.into(),
                    coin.owner.into(),
                    coin.amount.into(),
                    coin.asset_id.into(),
                    Default::default(),
                    0,
                    coin.maturity.into(),
                )),
                CoinType::MessageCoin(message) => {
                    tx.add_input(Input::message_coin_signed(
                        message.sender.into(),
                        message.recipient.into(),
                        message.amount.into(),
                        message.nonce.into(),
                        0,
                    ))
                }
                CoinType::Unknown => panic!("Unknown coin"),
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
                        tx_pointer_block_height: None,
                        tx_pointer_tx_idx: None,
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
        let mut nonce = 0;
        for owner in all_owners.iter() {
            messages.extend(vec![(owner, 60), (owner, 90)].into_iter().map(
                |(owner, amount)| {
                    let message = MessageConfig {
                        sender: *owner,
                        recipient: *owner,
                        nonce: (nonce as u64).into(),
                        amount,
                        data: vec![],
                        da_height: DaBlockHeight::from(1usize),
                    };
                    nonce += 1;
                    message
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
