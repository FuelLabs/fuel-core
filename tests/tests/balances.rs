use fuel_core::{
    chain_config::{
        CoinConfig,
        CoinConfigGenerator,
        MessageConfig,
        StateConfig,
    },
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
    types::{
        primitives::{
            Address,
            AssetId,
        },
        CoinType,
    },
    FuelClient,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_tx::{
        Input,
        Output,
        TransactionBuilder,
    },
};

#[tokio::test]
async fn balance() {
    let owner = Address::default();
    let asset_id = AssetId::BASE;

    // setup config
    let mut coin_generator = CoinConfigGenerator::new();
    let state_config = StateConfig {
        contracts: vec![],
        coins: vec![
            (owner, 50, asset_id),
            (owner, 100, asset_id),
            (owner, 150, asset_id),
        ]
        .into_iter()
        .map(|(owner, amount, asset_id)| CoinConfig {
            owner,
            amount,
            asset_id,
            ..coin_generator.generate()
        })
        .collect(),
        messages: vec![(owner, 60), (owner, 90)]
            .into_iter()
            .enumerate()
            .map(|(nonce, (owner, amount))| MessageConfig {
                sender: owner,
                recipient: owner,
                nonce: (nonce as u64).into(),
                amount,
                data: vec![],
                da_height: DaBlockHeight::from(0usize),
            })
            .collect(),
        ..Default::default()
    };
    let config = Config::local_node_with_state_config(state_config);

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    let balance = client.balance(&owner, Some(&asset_id)).await.unwrap();
    assert_eq!(balance, 450);

    // spend some coins and check again
    let coins_per_asset = client
        .coins_to_spend(&owner, vec![(asset_id, 1, None)], None)
        .await
        .unwrap();

    let mut tx = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(1_000_000)
        .to_owned();
    for coins in coins_per_asset {
        for coin in coins {
            match coin {
                CoinType::Coin(coin) => tx.add_input(Input::coin_signed(
                    coin.utxo_id,
                    coin.owner,
                    coin.amount,
                    coin.asset_id,
                    Default::default(),
                    0,
                )),
                CoinType::MessageCoin(message) => {
                    tx.add_input(Input::message_coin_signed(
                        message.sender,
                        message.recipient,
                        message.amount,
                        message.nonce,
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

    let balance = client.balance(&owner, Some(&asset_id)).await.unwrap();
    assert_eq!(balance, 449);
}

#[tokio::test]
async fn balance_messages_only() {
    let owner = Address::default();
    let asset_id = AssetId::BASE;

    const RETRYABLE: &[u8] = &[1];
    const NON_RETRYABLE: &[u8] = &[];

    // setup config
    let state_config = StateConfig {
        contracts: vec![],
        coins: vec![],
        messages: vec![
            (owner, 60, NON_RETRYABLE),
            (owner, 200, RETRYABLE),
            (owner, 90, NON_RETRYABLE),
        ]
        .into_iter()
        .enumerate()
        .map(|(nonce, (owner, amount, data))| MessageConfig {
            sender: owner,
            recipient: owner,
            nonce: (nonce as u64).into(),
            amount,
            data: data.to_vec(),
            da_height: DaBlockHeight::from(0usize),
        })
        .collect(),
        ..Default::default()
    };
    let config = Config::local_node_with_state_config(state_config);

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    const NON_RETRYABLE_AMOUNT: u64 = 60 + 90;
    let balance = client.balance(&owner, Some(&asset_id)).await.unwrap();
    assert_eq!(balance, NON_RETRYABLE_AMOUNT);
}

#[tokio::test]
async fn balances_messages_only() {
    let owner = Address::default();

    const RETRYABLE: &[u8] = &[1];
    const NON_RETRYABLE: &[u8] = &[];

    // setup config
    let state_config = StateConfig {
        contracts: vec![],
        coins: vec![],
        messages: vec![
            (owner, 60, NON_RETRYABLE),
            (owner, 200, RETRYABLE),
            (owner, 90, NON_RETRYABLE),
        ]
        .into_iter()
        .enumerate()
        .map(|(nonce, (owner, amount, data))| MessageConfig {
            sender: owner,
            recipient: owner,
            nonce: (nonce as u64).into(),
            amount,
            data: data.to_vec(),
            da_height: DaBlockHeight::from(0usize),
        })
        .collect(),
        ..Default::default()
    };
    let config = Config::local_node_with_state_config(state_config);

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    const NON_RETRYABLE_AMOUNT: u128 = 60 + 90;
    let balances = client
        .balances(
            &owner,
            PaginationRequest {
                cursor: None,
                results: 10,
                direction: PageDirection::Forward,
            },
        )
        .await
        .unwrap();
    assert_eq!(balances.results.len(), 1);
    let messages_balance = balances.results[0].amount;
    assert_eq!(messages_balance, NON_RETRYABLE_AMOUNT);
}

#[tokio::test]
async fn first_5_balances() {
    let owner = Address::from([10u8; 32]);
    let asset_ids = (0..=5u8)
        .map(|i| AssetId::new([i; 32]))
        .collect::<Vec<AssetId>>();

    let all_owners = [Address::default(), owner, Address::from([20u8; 32])];
    let coins = {
        // setup all coins for all owners
        let mut coin_generator = CoinConfigGenerator::new();
        let mut coins = vec![];
        for owner in all_owners.iter() {
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
                        owner: *owner,
                        amount,
                        asset_id,
                        ..coin_generator.generate()
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
                        da_height: DaBlockHeight::from(0usize),
                    };
                    nonce += 1;
                    message
                },
            ))
        }
        messages
    };

    // setup config
    let state_config = StateConfig {
        contracts: vec![],
        coins,
        messages,
        ..Default::default()
    };
    let config = Config::local_node_with_state_config(state_config);

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // run test
    let balances = client
        .balances(
            &owner,
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
    assert_eq!(balances[0].asset_id, asset_ids[0]);
    assert_eq!(balances[0].amount, 450);

    // Other assets are 3 coins = 50 + 100 + 150
    for i in 1..5 {
        assert_eq!(balances[i].asset_id, asset_ids[i]);
        assert_eq!(balances[i].amount, 300);
    }
}

mod pagination {
    use fuel_core::{
        chain_config::{
            CoinConfig,
            CoinConfigGenerator,
            StateConfig,
        },
        service::Config,
    };
    use fuel_core_bin::FuelService;
    use fuel_core_client::client::{
        pagination::{
            PageDirection,
            PaginationRequest,
        },
        FuelClient,
    };
    use fuel_core_types::fuel_tx::{
        Address,
        AssetId,
    };

    async fn setup(owner: &Address, asset_ids: &[AssetId]) -> Config {
        let coins = {
            // setup all coins for all owners
            let mut coin_generator = CoinConfigGenerator::new();
            let mut coins = vec![];
            coins.extend(
                asset_ids
                    .into_iter()
                    .enumerate()
                    .flat_map(|(i, asset_id)| vec![(owner, (i + 1) * 11, asset_id)])
                    .map(|(owner, amount, asset_id)| CoinConfig {
                        owner: *owner,
                        amount: amount as u64,
                        asset_id: *asset_id,
                        ..coin_generator.generate()
                    }),
            );
            coins
        };

        // setup config
        let state_config = StateConfig {
            contracts: vec![],
            coins,
            messages: vec![],
            ..Default::default()
        };
        Config::local_node_with_state_config(state_config)
    }

    #[tokio::test]
    async fn balances_pagination_without_base_asset() {
        // Given

        // Setup coins:
        // - single owner (aaaa...) has
        // - 11 units of asset (1111...)
        // - 22 units of asset (2222...)
        // - 33 units of asset (3333...)
        const TOTAL_ASSETS: usize = 3;
        let owner = Address::from([0xaa; 32]);
        let asset_1 = AssetId::new([0x11; 32]);
        let asset_2 = AssetId::new([0x22; 32]);
        let asset_3 = AssetId::new([0x33; 32]);
        let asset_ids = [asset_1, asset_2, asset_3];

        let config = setup(&owner, &asset_ids).await;
        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        // When
        let mut cursor = None;
        let mut actual_next_pages = vec![];
        let mut actual_balances = vec![];
        for _ in 0..TOTAL_ASSETS {
            let result = client
                .balances(
                    &owner,
                    PaginationRequest {
                        cursor,
                        results: 1,
                        direction: PageDirection::Forward,
                    },
                )
                .await
                .unwrap();
            assert_eq!(result.results.len(), 1);
            let first_and_only_result = result.results.first().unwrap();
            actual_balances
                .push((first_and_only_result.asset_id, first_and_only_result.amount));
            actual_next_pages.push(result.has_next_page);
            cursor = result.cursor;
        }

        // Then
        let expected_next_pages = [true, true, false];
        assert_eq!(expected_next_pages, actual_next_pages.as_slice());

        let expected_balances = vec![(asset_1, 11), (asset_2, 22), (asset_3, 33)];
        assert_eq!(expected_balances, actual_balances);
    }
}
