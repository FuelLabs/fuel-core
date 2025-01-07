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

const RETRYABLE: &[u8] = &[1];
const NON_RETRYABLE: &[u8] = &[];

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
        messages: vec![
            (owner, 60, NON_RETRYABLE),
            (owner, 90, NON_RETRYABLE),
            (owner, 200000, RETRYABLE),
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

    // Note that the big (200000)  message, which is RETRYABLE is not included in the balance
    assert_eq!(balance, 449);
}

#[tokio::test]
async fn balance_messages_only() {
    let owner = Address::default();
    let asset_id = AssetId::BASE;

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
            ChainConfig,
            CoinConfig,
            CoinConfigGenerator,
            MessageConfig,
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
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        fuel_tx::{
            Address,
            AssetId,
            ConsensusParameters,
        },
    };
    use test_case::test_matrix;

    async fn setup(
        owner: &Address,
        coin: &[(AssetId, u128)],
        message_amount: Option<u64>,
        base_asset_id: AssetId,
    ) -> Config {
        let coins = {
            // setup all coins for all owners
            let mut coin_generator = CoinConfigGenerator::new();
            let mut coins = vec![];
            coins.extend(
                coin.iter()
                    .flat_map(|(asset_id, amount)| vec![(owner, amount, asset_id)])
                    .map(|(owner, amount, asset_id)| CoinConfig {
                        owner: *owner,
                        amount: *amount as u64,
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
            messages: message_amount.map_or_else(Vec::new, |amount| {
                vec![MessageConfig {
                    sender: *owner,
                    recipient: *owner,
                    nonce: 1.into(),
                    amount,
                    data: vec![],
                    da_height: DaBlockHeight::from(0usize),
                }]
            }),
            ..Default::default()
        };

        // setup chain config
        let mut cp = ConsensusParameters::default();
        cp.set_base_asset_id(base_asset_id);

        let chain_config = ChainConfig::local_testnet_with_consensus_parameters(&cp);
        Config::local_node_with_configs(chain_config, state_config)
    }

    enum BaseAssetCoin {
        Present,
        Missing,
    }

    enum MessageCoin {
        Present,
        Missing,
    }

    const MESSAGE_BALANCE: u64 = 44;

    #[test_matrix(
        [PageDirection::Forward, PageDirection::Backward],
        [MessageCoin::Missing, MessageCoin::Present],
        [BaseAssetCoin::Present, BaseAssetCoin::Missing],
        [1, 2, 3, 2137],
        [0x11, 0x33, 0x99])]
    #[tokio::test]
    async fn all_balances_in_chunks(
        direction: PageDirection,
        message_coin: MessageCoin,
        base_asset_coin: BaseAssetCoin,
        chunk_size: i32,
        base_asset_id_byte: u8,
    ) {
        // Given

        // Owner has the following assets:
        // |   asset    | asset_id  | amount |  type   |         when?          |
        // | ---------- | --------- | ------ | ------- | ---------------------- |
        // | asset_1    | 0x2222... | 11     | coin    | always                 |
        // | asset_2    | 0x7777... | 22     | coin    | always                 |
        // | base_asset | 0x????... | 33     | coin    | BaseAssetCoin::Present |
        // | n/a        | 0x????... | 44     | message | MessageCoin::Present   |
        //
        // Please note that the lexicographical order of "base asset" is dependent on the test parameter,
        // so we can check for all three cases, i.e.: base asset is first, last or in the middle
        // of other assets, like so:
        // 1) base asset, asset_1, asset_2
        // 2) asset_1, base_asset, asset_2
        // 3) asset_1, asset_2, base_asset
        let base_asset_id = AssetId::from([base_asset_id_byte; 32]);

        let owner = Address::from([0xaa; 32]);
        let asset_1 = AssetId::new([0x22; 32]);
        let asset_2 = AssetId::new([0x77; 32]);
        let mut assets = vec![(asset_1, 11), (asset_2, 22)];
        if let BaseAssetCoin::Present = base_asset_coin {
            assets.push((base_asset_id, 33));
        }
        let config = setup(
            &owner,
            &assets,
            match message_coin {
                MessageCoin::Present => Some(MESSAGE_BALANCE),
                MessageCoin::Missing => None,
            },
            base_asset_id,
        )
        .await;
        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        // When
        let mut cursor = None;
        let mut actual_balances = vec![];
        loop {
            let paginated_result = client
                .balances(
                    &owner,
                    PaginationRequest {
                        cursor,
                        results: chunk_size,
                        direction,
                    },
                )
                .await
                .unwrap();
            assert!(paginated_result.results.len() <= chunk_size as usize);

            cursor = paginated_result.cursor;
            actual_balances.extend(
                paginated_result
                    .results
                    .iter()
                    .map(|r| (r.asset_id, r.amount)),
            );
            if !paginated_result.has_next_page {
                break
            }
        }

        // Then

        // Please mind that if present, base asset id is always reported first
        // (or last in case of backward pagination).
        let mut expected_balances = match (message_coin, base_asset_coin) {
            (MessageCoin::Missing, BaseAssetCoin::Missing) => {
                // Expect just regular coin balances
                vec![(asset_1, 11), (asset_2, 22)]
            }
            (MessageCoin::Missing, BaseAssetCoin::Present) => {
                // Expect regular coin balances + base asset
                vec![(base_asset_id, 33), (asset_1, 11), (asset_2, 22)]
            }
            (MessageCoin::Present, BaseAssetCoin::Missing) => {
                // Expect base asset id amount equal to message amount
                vec![(base_asset_id, 44), (asset_1, 11), (asset_2, 22)]
            }
            (MessageCoin::Present, BaseAssetCoin::Present) => {
                // Expect base asset id to be a sum of the message and base asset coin: 33 + 44 = 77
                vec![(base_asset_id, 77), (asset_1, 11), (asset_2, 22)]
            }
        };

        // If requesting backward, reverse the expected balances
        if direction == PageDirection::Backward {
            expected_balances.reverse();
        }

        assert_eq!(expected_balances, actual_balances);
    }

    #[test_matrix(
        [PageDirection::Forward, PageDirection::Backward],
        [0x11, 0x33, 0x99])]
    #[tokio::test]
    async fn no_balances_after_last_page(
        direction: PageDirection,
        base_asset_id_byte: u8,
    ) {
        let base_asset_id = AssetId::from([base_asset_id_byte; 32]);

        // Given
        let owner = Address::from([0xaa; 32]);
        let asset_1 = AssetId::new([0x22; 32]);
        let asset_2 = AssetId::new([0x77; 32]);
        let assets = vec![(asset_1, 11), (asset_2, 22), (base_asset_id, 33)];
        let config = setup(&owner, &assets, Some(MESSAGE_BALANCE), base_asset_id).await;
        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        // When
        let mut cursor = None;
        loop {
            let paginated_result = client
                .balances(
                    &owner,
                    PaginationRequest {
                        cursor,
                        results: 1,
                        direction,
                    },
                )
                .await
                .unwrap();

            cursor = paginated_result.cursor;
            if !paginated_result.has_next_page {
                break
            }
        }

        // Then
        let paginated_result = client
            .balances(
                &owner,
                PaginationRequest {
                    cursor,
                    results: 1,
                    direction,
                },
            )
            .await
            .unwrap();
        assert!(paginated_result.results.is_empty());
    }
}
