use crate::helpers::TestContext;
use fuel_core::{
    chain_config::{
        CoinConfig,
        MessageConfig,
        StateConfig,
    },
    coins_query::CoinsQueryError,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::CoinType,
    FuelClient,
};
use fuel_core_types::fuel_tx::*;
use rand::{
    prelude::StdRng,
    SeedableRng,
};

mod coin {
    use super::*;
    use fuel_core::chain_config::CoinConfigGenerator;
    use fuel_core_client::client::types::CoinType;
    use fuel_core_types::fuel_crypto::SecretKey;
    use rand::Rng;

    async fn setup(
        owner: Address,
        asset_id_a: AssetId,
        asset_id_b: AssetId,
    ) -> TestContext {
        // setup config
        let mut coin_generator = CoinConfigGenerator::new();
        let state = StateConfig {
            contracts: vec![],
            coins: vec![
                (owner, 50, asset_id_a),
                (owner, 100, asset_id_a),
                (owner, 150, asset_id_a),
                (owner, 50, asset_id_b),
                (owner, 100, asset_id_b),
                (owner, 150, asset_id_b),
            ]
            .into_iter()
            .map(|(owner, amount, asset_id)| CoinConfig {
                owner,
                amount,
                asset_id,
                ..coin_generator.generate()
            })
            .collect(),
            messages: vec![],
            ..Default::default()
        };
        let config = Config::local_node_with_state_config(state);

        // setup server & client
        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        TestContext {
            srv,
            rng: StdRng::seed_from_u64(0x123),
            client,
        }
    }

    #[rstest::rstest]
    #[tokio::test]
    async fn coins_to_spend(
        #[values(Address::default(), Address::from([5; 32]), Address::from([16; 32]))]
        owner: Address,
        #[values(AssetId::new([16u8; 32]), AssetId::new([1u8; 32]))] asset_id_a: AssetId,
        #[values(AssetId::new([2u8; 32]), AssetId::new([99u8; 32]))] asset_id_b: AssetId,
    ) {
        query_target_1(owner, asset_id_a, asset_id_b).await;
        query_target_300(owner, asset_id_a, asset_id_b).await;
        exclude_all(owner, asset_id_a, asset_id_b).await;
        query_more_than_we_have(owner, asset_id_a, asset_id_b).await;
        query_limit_coins(owner, asset_id_a, asset_id_b).await;
    }

    #[tokio::test]
    async fn excludes_spent_coins() {
        let mut rng = StdRng::seed_from_u64(1234);
        let asset_id_a: AssetId = rng.gen();
        let asset_id_b: AssetId = rng.gen();
        let secret_key: SecretKey = SecretKey::random(&mut rng);
        let pk = secret_key.public_key();
        let owner = Input::owner(&pk);
        let context = setup(owner, asset_id_a, asset_id_b).await;
        // select all available coins to spend
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 300, None), (asset_id_b, 300, None)],
                None,
            )
            .await
            .unwrap();

        // spend all coins
        let mut script = TransactionBuilder::script(vec![], vec![]);

        for asset_group in coins_per_asset {
            for asset in asset_group {
                if let CoinType::Coin(coin) = asset {
                    script.add_unsigned_coin_input(
                        secret_key,
                        coin.utxo_id,
                        coin.amount,
                        coin.asset_id,
                        Default::default(),
                    );
                }
            }
        }
        // send change to different address
        script.add_output(Output::change(rng.gen(), 0, asset_id_a));
        script.add_output(Output::change(rng.gen(), 0, asset_id_b));
        let tx = script.finalize_as_transaction();

        context.client.submit_and_await_commit(&tx).await.unwrap();

        // select all available asset a coins to spend
        let remaining_coins_a = context
            .client
            .coins_to_spend(&owner, vec![(asset_id_a, 1, None)], None)
            .await;
        // there should be none left
        assert!(remaining_coins_a.is_err());

        // select all available asset a coins to spend
        let remaining_coins_b = context
            .client
            .coins_to_spend(&owner, vec![(asset_id_b, 1, None)], None)
            .await;
        // there should be none left
        assert!(remaining_coins_b.is_err())
    }

    async fn query_target_1(owner: Address, asset_id_a: AssetId, asset_id_b: AssetId) {
        let context = setup(owner, asset_id_a, asset_id_b).await;

        // spend_query for 1 a and 1 b
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 1, None), (asset_id_b, 1, None)],
                None,
            )
            .await
            .unwrap();
        assert_eq!(coins_per_asset.len(), 2);
        assert_eq!(coins_per_asset[0].len(), 1);
        assert!(coins_per_asset[0].amount() >= 1);
        assert_eq!(coins_per_asset[1].len(), 1);
        assert!(coins_per_asset[1].amount() >= 1);
    }

    async fn query_target_300(owner: Address, asset_id_a: AssetId, asset_id_b: AssetId) {
        let context = setup(owner, asset_id_a, asset_id_b).await;

        // spend_query for 300 a and 300 b
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 300, None), (asset_id_b, 300, None)],
                None,
            )
            .await
            .unwrap();
        assert_eq!(coins_per_asset.len(), 2);
        assert_eq!(coins_per_asset[0].len(), 3);
        assert!(coins_per_asset[0].amount() >= 300);
        assert_eq!(coins_per_asset[1].len(), 3);
        assert!(coins_per_asset[1].amount() >= 300);
    }

    async fn exclude_all(owner: Address, asset_id_a: AssetId, asset_id_b: AssetId) {
        let context = setup(owner, asset_id_a, asset_id_b).await;

        // query all coins
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 300, None), (asset_id_b, 300, None)],
                None,
            )
            .await
            .unwrap();

        // spend_query for 1 a and 1 b, but with all coins excluded
        let all_utxos = coins_per_asset
            .iter()
            .flat_map(|coins| {
                coins.iter().filter_map(|b| match b {
                    CoinType::Coin(c) => Some(c.utxo_id),
                    CoinType::MessageCoin(_) => None,
                    CoinType::Unknown => None,
                })
            })
            .collect();
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 1, None), (asset_id_b, 1, None)],
                Some((all_utxos, vec![])),
            )
            .await;
        assert!(coins_per_asset.is_err());
        assert_eq!(
            coins_per_asset.unwrap_err().to_string(),
            CoinsQueryError::InsufficientCoins {
                asset_id: asset_id_a,
                collected_amount: 0,
            }
            .to_str_error_string()
        );
    }

    async fn query_more_than_we_have(
        owner: Address,
        asset_id_a: AssetId,
        asset_id_b: AssetId,
    ) {
        let context = setup(owner, asset_id_a, asset_id_b).await;

        // not enough coins
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 301, None), (asset_id_b, 301, None)],
                None,
            )
            .await;
        assert!(coins_per_asset.is_err());
        assert_eq!(
            coins_per_asset.unwrap_err().to_string(),
            CoinsQueryError::InsufficientCoins {
                asset_id: asset_id_a,
                collected_amount: 300,
            }
            .to_str_error_string()
        );
    }

    async fn query_limit_coins(owner: Address, asset_id_a: AssetId, asset_id_b: AssetId) {
        let context = setup(owner, asset_id_a, asset_id_b).await;

        // not enough inputs
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 300, Some(2)), (asset_id_b, 300, Some(2))],
                None,
            )
            .await;
        assert!(coins_per_asset.is_err());
        assert_eq!(
            coins_per_asset.unwrap_err().to_string(),
            CoinsQueryError::MaxCoinsReached.to_str_error_string()
        );
    }
}

mod message_coin {
    use fuel_core_client::client::types::CoinType;
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        fuel_crypto::SecretKey,
    };
    use rand::Rng;

    use super::*;

    async fn setup(owner: Address) -> (AssetId, TestContext) {
        let base_asset_id = AssetId::BASE;

        // setup config
        let state = StateConfig {
            contracts: vec![],
            coins: vec![],
            messages: vec![(owner, 50), (owner, 100), (owner, 150)]
                .into_iter()
                .enumerate()
                .map(|(nonce, (owner, amount))| MessageConfig {
                    sender: owner,
                    recipient: owner,
                    nonce: (nonce as u64).into(),
                    amount,
                    data: vec![],
                    da_height: DaBlockHeight::from(0u64),
                })
                .collect(),
            ..Default::default()
        };
        let config = Config::local_node_with_state_config(state);

        // setup server & client
        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);
        let context = TestContext {
            srv,
            rng: StdRng::seed_from_u64(0x123),
            client,
        };

        (base_asset_id, context)
    }

    #[rstest::rstest]
    #[tokio::test]
    async fn coins_to_spend(
        #[values(Address::default(), Address::from([5; 32]), Address::from([16; 32]))]
        owner: Address,
    ) {
        query_target_1(owner).await;
        query_target_300(owner).await;
        exclude_all(owner).await;
        query_more_than_we_have(owner).await;
        query_limit_coins(owner).await;
    }

    #[tokio::test]
    async fn excludes_spent_coins() {
        let mut rng = StdRng::seed_from_u64(1234);

        let secret_key: SecretKey = SecretKey::random(&mut rng);
        let pk = secret_key.public_key();
        let owner = Input::owner(&pk);
        let (base_asset_id, context) = setup(owner).await;
        // select all available coins to spend
        let coins_per_asset = context
            .client
            .coins_to_spend(&owner, vec![(base_asset_id, 300, None)], None)
            .await
            .unwrap();

        // spend all coins
        let mut script = TransactionBuilder::script(vec![], vec![]);

        coins_per_asset[0].iter().for_each(|coin| {
            if let CoinType::MessageCoin(message) = coin {
                script.add_unsigned_message_input(
                    secret_key,
                    message.sender,
                    message.nonce,
                    message.amount,
                    vec![],
                );
            }
        });
        // send change to different address
        script.add_output(Output::change(rng.gen(), 0, base_asset_id));
        let tx = script.finalize_as_transaction();

        context.client.submit_and_await_commit(&tx).await.unwrap();

        // select all available coins to spend
        let remaining_coins = context
            .client
            .coins_to_spend(&owner, vec![(base_asset_id, 1, None)], None)
            .await;
        // there should be none left
        assert!(remaining_coins.is_err())
    }

    async fn query_target_1(owner: Address) {
        let (base_asset_id, context) = setup(owner).await;

        // query coins for `base_asset_id` and target 1
        let coins_per_asset = context
            .client
            .coins_to_spend(&owner, vec![(base_asset_id, 1, None)], None)
            .await
            .unwrap();
        assert_eq!(coins_per_asset.len(), 1);
    }

    async fn query_target_300(owner: Address) {
        let (base_asset_id, context) = setup(owner).await;

        // query for 300 base assets
        let coins_per_asset = context
            .client
            .coins_to_spend(&owner, vec![(base_asset_id, 300, None)], None)
            .await
            .unwrap();
        assert_eq!(coins_per_asset.len(), 1);
        assert_eq!(coins_per_asset[0].len(), 3);
    }

    async fn exclude_all(owner: Address) {
        let (base_asset_id, context) = setup(owner).await;

        // query for 300 base assets
        let coins_per_asset = context
            .client
            .coins_to_spend(&owner, vec![(base_asset_id, 300, None)], None)
            .await
            .unwrap();

        // query base assets, but with all coins excluded
        let all_message_ids = coins_per_asset
            .iter()
            .flat_map(|coins| {
                coins.iter().filter_map(|b| match b {
                    CoinType::Coin(_) => None,
                    CoinType::MessageCoin(m) => Some(m.nonce),
                    CoinType::Unknown => None,
                })
            })
            .collect();
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(base_asset_id, 1, None)],
                Some((vec![], all_message_ids)),
            )
            .await;
        assert!(coins_per_asset.is_err());
        assert_eq!(
            coins_per_asset.unwrap_err().to_string(),
            CoinsQueryError::InsufficientCoins {
                asset_id: base_asset_id,
                collected_amount: 0,
            }
            .to_str_error_string()
        );
    }

    async fn query_more_than_we_have(owner: Address) {
        let (base_asset_id, context) = setup(owner).await;

        // max coins reached
        let coins_per_asset = context
            .client
            .coins_to_spend(&owner, vec![(base_asset_id, 301, None)], None)
            .await;
        assert!(coins_per_asset.is_err());
        assert_eq!(
            coins_per_asset.unwrap_err().to_string(),
            CoinsQueryError::InsufficientCoins {
                asset_id: base_asset_id,
                collected_amount: 300,
            }
            .to_str_error_string()
        );
    }

    async fn query_limit_coins(owner: Address) {
        let (base_asset_id, context) = setup(owner).await;

        // not enough inputs
        let coins_per_asset = context
            .client
            .coins_to_spend(&owner, vec![(base_asset_id, 300, Some(2))], None)
            .await;
        assert!(coins_per_asset.is_err());
        assert_eq!(
            coins_per_asset.unwrap_err().to_string(),
            CoinsQueryError::MaxCoinsReached.to_str_error_string()
        );
    }
}

// It is combination of coins and deposit coins test cases.
mod all_coins {
    use fuel_core::chain_config::CoinConfigGenerator;
    use fuel_core_client::client::types::CoinType;
    use fuel_core_types::blockchain::primitives::DaBlockHeight;

    use super::*;

    async fn setup(owner: Address, asset_id_b: AssetId) -> (AssetId, TestContext) {
        let asset_id_a = AssetId::BASE;

        // setup config
        let mut coin_generator = CoinConfigGenerator::new();
        let state = StateConfig {
            contracts: vec![],
            coins: vec![
                (owner, 100, asset_id_a),
                (owner, 50, asset_id_b),
                (owner, 100, asset_id_b),
                (owner, 150, asset_id_b),
            ]
            .into_iter()
            .map(|(owner, amount, asset_id)| CoinConfig {
                owner,
                amount,
                asset_id,
                ..coin_generator.generate()
            })
            .collect(),
            messages: vec![(owner, 50), (owner, 150)]
                .into_iter()
                .enumerate()
                .map(|(nonce, (owner, amount))| MessageConfig {
                    sender: owner,
                    recipient: owner,
                    nonce: (nonce as u64).into(),
                    amount,
                    data: vec![],
                    da_height: DaBlockHeight::from(0u64),
                })
                .collect(),
            ..Default::default()
        };
        let config = Config::local_node_with_state_config(state);

        // setup server & client
        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);
        let context = TestContext {
            srv,
            rng: StdRng::seed_from_u64(0x123),
            client,
        };

        (asset_id_a, context)
    }

    #[rstest::rstest]
    #[tokio::test]
    async fn coins_to_spend(
        #[values(Address::default(), Address::from([5; 32]), Address::from([16; 32]))]
        owner: Address,
        #[values(AssetId::new([1u8; 32]), AssetId::new([99u8; 32]))] asset_id_b: AssetId,
    ) {
        query_target_1(owner, asset_id_b).await;
        query_target_300(owner, asset_id_b).await;
        exclude_all(owner, asset_id_b).await;
        query_more_than_we_have(owner, asset_id_b).await;
        query_limit_coins(owner, asset_id_b).await;
    }

    async fn query_target_1(owner: Address, asset_id_b: AssetId) {
        let (asset_id_a, context) = setup(owner, asset_id_b).await;

        // query coins for `base_asset_id` and target 1
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 1, None), (asset_id_b, 1, None)],
                None,
            )
            .await
            .unwrap();
        assert_eq!(coins_per_asset.len(), 2);
        assert_eq!(coins_per_asset[0].len(), 1);
        assert!(coins_per_asset[0].amount() >= 1);
        assert_eq!(coins_per_asset[1].len(), 1);
        assert!(coins_per_asset[1].amount() >= 1);
    }

    async fn query_target_300(owner: Address, asset_id_b: AssetId) {
        let (asset_id_a, context) = setup(owner, asset_id_b).await;

        // query for 300 base assets
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 300, None), (asset_id_b, 300, None)],
                None,
            )
            .await
            .unwrap();
        assert_eq!(coins_per_asset.len(), 2);
        assert_eq!(coins_per_asset[0].len(), 3);
        assert!(coins_per_asset[0].amount() >= 300);
        assert_eq!(coins_per_asset[1].len(), 3);
        assert!(coins_per_asset[1].amount() >= 300);
    }

    async fn exclude_all(owner: Address, asset_id_b: AssetId) {
        let (asset_id_a, context) = setup(owner, asset_id_b).await;

        // query for 300 base assets
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 300, None), (asset_id_b, 300, None)],
                None,
            )
            .await
            .unwrap();

        // query base assets, but with all coins excluded
        let all_message_ids: Vec<_> = coins_per_asset
            .iter()
            .flat_map(|coins| {
                coins.iter().filter_map(|b| match b {
                    CoinType::Coin(_) => None,
                    CoinType::MessageCoin(m) => Some(m.nonce),
                    CoinType::Unknown => None,
                })
            })
            .collect();
        let all_utxo_ids: Vec<_> = coins_per_asset
            .iter()
            .flat_map(|coins| {
                coins.iter().filter_map(|b| match b {
                    CoinType::Coin(c) => Some(c.utxo_id),
                    CoinType::MessageCoin(_) => None,
                    CoinType::Unknown => None,
                })
            })
            .collect();

        // After setup we have 4 `Coin`s and 2 `Message`s
        assert_eq!(all_utxo_ids.len(), 4);
        assert_eq!(all_message_ids.len(), 2);

        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 1, None), (asset_id_b, 1, None)],
                Some((all_utxo_ids, all_message_ids)),
            )
            .await;
        assert!(coins_per_asset.is_err());
        assert_eq!(
            coins_per_asset.unwrap_err().to_string(),
            CoinsQueryError::InsufficientCoins {
                asset_id: asset_id_a,
                collected_amount: 0,
            }
            .to_str_error_string()
        );
    }

    async fn query_more_than_we_have(owner: Address, asset_id_b: AssetId) {
        let (asset_id_a, context) = setup(owner, asset_id_b).await;

        // max coins reached
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 301, None), (asset_id_b, 301, None)],
                None,
            )
            .await;
        assert!(coins_per_asset.is_err());
        assert_eq!(
            coins_per_asset.unwrap_err().to_string(),
            CoinsQueryError::InsufficientCoins {
                asset_id: asset_id_a,
                collected_amount: 300,
            }
            .to_str_error_string()
        );
    }

    async fn query_limit_coins(owner: Address, asset_id_b: AssetId) {
        let (asset_id_a, context) = setup(owner, asset_id_b).await;

        // not enough inputs
        let coins_per_asset = context
            .client
            .coins_to_spend(
                &owner,
                vec![(asset_id_a, 300, Some(2)), (asset_id_b, 300, Some(2))],
                None,
            )
            .await;
        assert!(coins_per_asset.is_err());
        assert_eq!(
            coins_per_asset.unwrap_err().to_string(),
            CoinsQueryError::MaxCoinsReached.to_str_error_string()
        );
    }
}

async fn empty_setup() -> TestContext {
    // setup config
    let config = Config::local_node_with_state_config(StateConfig::default());

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    TestContext {
        srv,
        rng: StdRng::seed_from_u64(0x123),
        client,
    }
}

#[rstest::rstest]
#[tokio::test]
async fn coins_to_spend_empty(
    #[values(Address::default(), Address::from([5; 32]), Address::from([16; 32]))]
    owner: Address,
) {
    let context = empty_setup().await;

    // empty spend_query
    let coins_per_asset = context
        .client
        .coins_to_spend(&owner, vec![], None)
        .await
        .unwrap();
    assert!(coins_per_asset.is_empty());
}

#[rstest::rstest]
#[tokio::test]
async fn coins_to_spend_error_duplicate_asset_query(
    #[values(Address::default(), Address::from([5; 32]), Address::from([16; 32]))]
    owner: Address,
    #[values(AssetId::new([1u8; 32]), AssetId::new([99u8; 32]))] asset_id: AssetId,
) {
    let context = empty_setup().await;

    // the queries with the same id
    let coins_per_asset = context
        .client
        .coins_to_spend(
            &owner,
            vec![
                (asset_id, 1, None),
                (asset_id, 2, None),
                (asset_id, 3, None),
            ],
            None,
        )
        .await;
    assert!(coins_per_asset.is_err());
    assert_eq!(
        coins_per_asset.unwrap_err().to_string(),
        CoinsQueryError::DuplicateAssets(asset_id).to_str_error_string()
    );
}

trait ToStdErrorString {
    fn to_str_error_string(self) -> String;
}

impl ToStdErrorString for CoinsQueryError {
    fn to_str_error_string(self) -> String {
        fuel_core_client::client::from_strings_errors_to_std_error(vec![self.to_string()])
            .to_string()
    }
}

trait CumulativeAmount {
    fn amount(&self) -> u64;
}

impl CumulativeAmount for Vec<CoinType> {
    fn amount(&self) -> u64 {
        self.iter().map(|coin| coin.amount()).sum()
    }
}
