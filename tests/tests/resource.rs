use crate::helpers::TestContext;
use fuel_core::{
    chain_config::{
        CoinConfig,
        MessageConfig,
        StateConfig,
    },
    resource_query::ResourceQueryError,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    schema::resource::Resource,
    FuelClient,
};
use fuel_core_types::fuel_tx::*;
use rand::{
    prelude::StdRng,
    SeedableRng,
};

mod coins {
    use super::*;

    async fn setup(
        owner: Address,
        asset_id_a: AssetId,
        asset_id_b: AssetId,
    ) -> TestContext {
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
            messages: None,
        });

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
    async fn resources_to_spend(
        #[values(Address::default(), Address::from([5; 32]), Address::from([16; 32]))]
        owner: Address,
        #[values(AssetId::new([16u8; 32]), AssetId::new([1u8; 32]))] asset_id_a: AssetId,
        #[values(AssetId::new([2u8; 32]), AssetId::new([99u8; 32]))] asset_id_b: AssetId,
    ) {
        query_target_1(owner, asset_id_a, asset_id_b).await;
        query_target_300(owner, asset_id_a, asset_id_b).await;
        exclude_all(owner, asset_id_a, asset_id_b).await;
        query_more_than_we_have(owner, asset_id_a, asset_id_b).await;
        query_limit_resources(owner, asset_id_a, asset_id_b).await;
    }

    async fn query_target_1(owner: Address, asset_id_a: AssetId, asset_id_b: AssetId) {
        let context = setup(owner, asset_id_a, asset_id_b).await;

        // spend_query for 1 a and 1 b
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![
                    (format!("{asset_id_a:#x}").as_str(), 1, None),
                    (format!("{asset_id_b:#x}").as_str(), 1, None),
                ],
                None,
            )
            .await
            .unwrap();
        assert_eq!(resources_per_asset.len(), 2);
        assert_eq!(resources_per_asset[0].len(), 1);
        assert!(resources_per_asset[0].amount() >= 1);
        assert_eq!(resources_per_asset[1].len(), 1);
        assert!(resources_per_asset[1].amount() >= 1);
    }

    async fn query_target_300(owner: Address, asset_id_a: AssetId, asset_id_b: AssetId) {
        let context = setup(owner, asset_id_a, asset_id_b).await;

        // spend_query for 300 a and 300 b
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![
                    (format!("{asset_id_a:#x}").as_str(), 300, None),
                    (format!("{asset_id_b:#x}").as_str(), 300, None),
                ],
                None,
            )
            .await
            .unwrap();
        assert_eq!(resources_per_asset.len(), 2);
        assert_eq!(resources_per_asset[0].len(), 3);
        assert!(resources_per_asset[0].amount() >= 300);
        assert_eq!(resources_per_asset[1].len(), 3);
        assert!(resources_per_asset[1].amount() >= 300);
    }

    async fn exclude_all(owner: Address, asset_id_a: AssetId, asset_id_b: AssetId) {
        let context = setup(owner, asset_id_a, asset_id_b).await;

        // query all resources
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![
                    (format!("{asset_id_a:#x}").as_str(), 300, None),
                    (format!("{asset_id_b:#x}").as_str(), 300, None),
                ],
                None,
            )
            .await
            .unwrap();

        // spend_query for 1 a and 1 b, but with all resources excluded
        let all_utxos: Vec<String> = resources_per_asset
            .iter()
            .flat_map(|resources| {
                resources.iter().filter_map(|b| match b {
                    Resource::Coin(c) => Some(format!("{:#x}", c.utxo_id)),
                    Resource::Message(_) => None,
                    Resource::Unknown => None,
                })
            })
            .collect();
        let all_utxo_ids = all_utxos.iter().map(String::as_str).collect();
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![
                    (format!("{asset_id_a:#x}").as_str(), 1, None),
                    (format!("{asset_id_b:#x}").as_str(), 1, None),
                ],
                Some((all_utxo_ids, vec![])),
            )
            .await;
        assert!(resources_per_asset.is_err());
        assert_eq!(
            resources_per_asset.unwrap_err().to_string(),
            ResourceQueryError::InsufficientResources {
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

        // not enough resources
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![
                    (format!("{asset_id_a:#x}").as_str(), 301, None),
                    (format!("{asset_id_b:#x}").as_str(), 301, None),
                ],
                None,
            )
            .await;
        assert!(resources_per_asset.is_err());
        assert_eq!(
            resources_per_asset.unwrap_err().to_string(),
            ResourceQueryError::InsufficientResources {
                asset_id: asset_id_a,
                collected_amount: 300,
            }
            .to_str_error_string()
        );
    }

    async fn query_limit_resources(
        owner: Address,
        asset_id_a: AssetId,
        asset_id_b: AssetId,
    ) {
        let context = setup(owner, asset_id_a, asset_id_b).await;

        // not enough inputs
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![
                    (format!("{asset_id_a:#x}").as_str(), 300, Some(2)),
                    (format!("{asset_id_b:#x}").as_str(), 300, Some(2)),
                ],
                None,
            )
            .await;
        assert!(resources_per_asset.is_err());
        assert_eq!(
            resources_per_asset.unwrap_err().to_string(),
            ResourceQueryError::MaxResourcesReached.to_str_error_string()
        );
    }
}

mod messages {
    use fuel_core_types::blockchain::primitives::DaBlockHeight;

    use super::*;

    async fn setup(owner: Address) -> (AssetId, TestContext) {
        let base_asset_id = AssetId::BASE;

        // setup config
        let mut config = Config::local_node();
        config.chain_conf.initial_state = Some(StateConfig {
            height: None,
            contracts: None,
            coins: None,
            messages: Some(
                vec![(owner, 50), (owner, 100), (owner, 150)]
                    .into_iter()
                    .enumerate()
                    .map(|(nonce, (owner, amount))| MessageConfig {
                        sender: owner,
                        recipient: owner,
                        nonce: nonce as u64,
                        amount,
                        data: vec![],
                        da_height: DaBlockHeight::from(1u64),
                    })
                    .collect(),
            ),
        });

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
    async fn resources_to_spend(
        #[values(Address::default(), Address::from([5; 32]), Address::from([16; 32]))]
        owner: Address,
    ) {
        query_target_1(owner).await;
        query_target_300(owner).await;
        exclude_all(owner).await;
        query_more_than_we_have(owner).await;
        query_limit_resources(owner).await;
    }

    async fn query_target_1(owner: Address) {
        let (base_asset_id, context) = setup(owner).await;

        // query resources for `base_asset_id` and target 1
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![(format!("{base_asset_id:#x}").as_str(), 1, None)],
                None,
            )
            .await
            .unwrap();
        assert_eq!(resources_per_asset.len(), 1);
    }

    async fn query_target_300(owner: Address) {
        let (base_asset_id, context) = setup(owner).await;

        // query for 300 base assets
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![(format!("{base_asset_id:#x}").as_str(), 300, None)],
                None,
            )
            .await
            .unwrap();
        assert_eq!(resources_per_asset.len(), 1);
        assert_eq!(resources_per_asset[0].len(), 3);
    }

    async fn exclude_all(owner: Address) {
        let (base_asset_id, context) = setup(owner).await;

        // query for 300 base assets
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![(format!("{base_asset_id:#x}").as_str(), 300, None)],
                None,
            )
            .await
            .unwrap();

        // query base assets, but with all resources excluded
        let all_message_ids: Vec<String> = resources_per_asset
            .iter()
            .flat_map(|resources| {
                resources.iter().filter_map(|b| match b {
                    Resource::Coin(_) => None,
                    Resource::Message(m) => Some(format!("{:#x}", m.message_id)),
                    Resource::Unknown => None,
                })
            })
            .collect();
        let all_message_ids = all_message_ids.iter().map(String::as_str).collect();
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![(format!("{base_asset_id:#x}").as_str(), 1, None)],
                Some((vec![], all_message_ids)),
            )
            .await;
        assert!(resources_per_asset.is_err());
        assert_eq!(
            resources_per_asset.unwrap_err().to_string(),
            ResourceQueryError::InsufficientResources {
                asset_id: base_asset_id,
                collected_amount: 0,
            }
            .to_str_error_string()
        );
    }

    async fn query_more_than_we_have(owner: Address) {
        let (base_asset_id, context) = setup(owner).await;

        // max resources reached
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![(format!("{base_asset_id:#x}").as_str(), 301, None)],
                None,
            )
            .await;
        assert!(resources_per_asset.is_err());
        assert_eq!(
            resources_per_asset.unwrap_err().to_string(),
            ResourceQueryError::InsufficientResources {
                asset_id: base_asset_id,
                collected_amount: 300,
            }
            .to_str_error_string()
        );
    }

    async fn query_limit_resources(owner: Address) {
        let (base_asset_id, context) = setup(owner).await;

        // not enough inputs
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![(format!("{base_asset_id:#x}").as_str(), 300, Some(2))],
                None,
            )
            .await;
        assert!(resources_per_asset.is_err());
        assert_eq!(
            resources_per_asset.unwrap_err().to_string(),
            ResourceQueryError::MaxResourcesReached.to_str_error_string()
        );
    }
}

// It is combination of coins and messages test cases.
mod messages_and_coins {
    use fuel_core_types::blockchain::primitives::DaBlockHeight;

    use super::*;

    async fn setup(owner: Address, asset_id_b: AssetId) -> (AssetId, TestContext) {
        let asset_id_a = AssetId::BASE;

        // setup config
        let mut config = Config::local_node();
        config.chain_conf.initial_state = Some(StateConfig {
            height: None,
            contracts: None,
            coins: Some(
                vec![
                    (owner, 100, asset_id_a),
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
            messages: Some(
                vec![(owner, 50), (owner, 150)]
                    .into_iter()
                    .enumerate()
                    .map(|(nonce, (owner, amount))| MessageConfig {
                        sender: owner,
                        recipient: owner,
                        nonce: nonce as u64,
                        amount,
                        data: vec![],
                        da_height: DaBlockHeight::from(1u64),
                    })
                    .collect(),
            ),
        });

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
    async fn resources_to_spend(
        #[values(Address::default(), Address::from([5; 32]), Address::from([16; 32]))]
        owner: Address,
        #[values(AssetId::new([1u8; 32]), AssetId::new([99u8; 32]))] asset_id_b: AssetId,
    ) {
        query_target_1(owner, asset_id_b).await;
        query_target_300(owner, asset_id_b).await;
        exclude_all(owner, asset_id_b).await;
        query_more_than_we_have(owner, asset_id_b).await;
        query_limit_resources(owner, asset_id_b).await;
    }

    async fn query_target_1(owner: Address, asset_id_b: AssetId) {
        let (asset_id_a, context) = setup(owner, asset_id_b).await;

        // query resources for `base_asset_id` and target 1
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![
                    (format!("{asset_id_a:#x}").as_str(), 1, None),
                    (format!("{asset_id_b:#x}").as_str(), 1, None),
                ],
                None,
            )
            .await
            .unwrap();
        assert_eq!(resources_per_asset.len(), 2);
        assert_eq!(resources_per_asset[0].len(), 1);
        assert!(resources_per_asset[0].amount() >= 1);
        assert_eq!(resources_per_asset[1].len(), 1);
        assert!(resources_per_asset[1].amount() >= 1);
    }

    async fn query_target_300(owner: Address, asset_id_b: AssetId) {
        let (asset_id_a, context) = setup(owner, asset_id_b).await;

        // query for 300 base assets
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![
                    (format!("{asset_id_a:#x}").as_str(), 300, None),
                    (format!("{asset_id_b:#x}").as_str(), 300, None),
                ],
                None,
            )
            .await
            .unwrap();
        assert_eq!(resources_per_asset.len(), 2);
        assert_eq!(resources_per_asset[0].len(), 3);
        assert!(resources_per_asset[0].amount() >= 300);
        assert_eq!(resources_per_asset[1].len(), 3);
        assert!(resources_per_asset[1].amount() >= 300);
    }

    async fn exclude_all(owner: Address, asset_id_b: AssetId) {
        let (asset_id_a, context) = setup(owner, asset_id_b).await;

        // query for 300 base assets
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![
                    (format!("{asset_id_a:#x}").as_str(), 300, None),
                    (format!("{asset_id_b:#x}").as_str(), 300, None),
                ],
                None,
            )
            .await
            .unwrap();

        // query base assets, but with all resources excluded
        let all_message_ids: Vec<String> = resources_per_asset
            .iter()
            .flat_map(|resources| {
                resources.iter().filter_map(|b| match b {
                    Resource::Coin(_) => None,
                    Resource::Message(m) => Some(format!("{:#x}", m.message_id)),
                    Resource::Unknown => None,
                })
            })
            .collect();
        let all_utxo_ids: Vec<String> = resources_per_asset
            .iter()
            .flat_map(|resources| {
                resources.iter().filter_map(|b| match b {
                    Resource::Coin(c) => Some(format!("{:#x}", c.utxo_id)),
                    Resource::Message(_) => None,
                    Resource::Unknown => None,
                })
            })
            .collect();

        let all_message_ids: Vec<_> =
            all_message_ids.iter().map(String::as_str).collect();
        let all_utxo_ids: Vec<_> = all_utxo_ids.iter().map(String::as_str).collect();

        // After setup we have 4 `Coin`s and 2 `Message`s
        assert_eq!(all_utxo_ids.len(), 4);
        assert_eq!(all_message_ids.len(), 2);

        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![
                    (format!("{asset_id_a:#x}").as_str(), 1, None),
                    (format!("{asset_id_b:#x}").as_str(), 1, None),
                ],
                Some((all_utxo_ids, all_message_ids)),
            )
            .await;
        assert!(resources_per_asset.is_err());
        assert_eq!(
            resources_per_asset.unwrap_err().to_string(),
            ResourceQueryError::InsufficientResources {
                asset_id: asset_id_a,
                collected_amount: 0,
            }
            .to_str_error_string()
        );
    }

    async fn query_more_than_we_have(owner: Address, asset_id_b: AssetId) {
        let (asset_id_a, context) = setup(owner, asset_id_b).await;

        // max resources reached
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![
                    (format!("{asset_id_a:#x}").as_str(), 301, None),
                    (format!("{asset_id_b:#x}").as_str(), 301, None),
                ],
                None,
            )
            .await;
        assert!(resources_per_asset.is_err());
        assert_eq!(
            resources_per_asset.unwrap_err().to_string(),
            ResourceQueryError::InsufficientResources {
                asset_id: asset_id_a,
                collected_amount: 300,
            }
            .to_str_error_string()
        );
    }

    async fn query_limit_resources(owner: Address, asset_id_b: AssetId) {
        let (asset_id_a, context) = setup(owner, asset_id_b).await;

        // not enough inputs
        let resources_per_asset = context
            .client
            .resources_to_spend(
                format!("{owner:#x}").as_str(),
                vec![
                    (format!("{asset_id_a:#x}").as_str(), 300, Some(2)),
                    (format!("{asset_id_b:#x}").as_str(), 300, Some(2)),
                ],
                None,
            )
            .await;
        assert!(resources_per_asset.is_err());
        assert_eq!(
            resources_per_asset.unwrap_err().to_string(),
            ResourceQueryError::MaxResourcesReached.to_str_error_string()
        );
    }
}

async fn empty_setup() -> TestContext {
    // setup config
    let mut config = Config::local_node();
    config.chain_conf.initial_state = Some(StateConfig {
        height: None,
        contracts: None,
        coins: None,
        messages: None,
    });

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
async fn resources_to_spend_empty(
    #[values(Address::default(), Address::from([5; 32]), Address::from([16; 32]))]
    owner: Address,
) {
    let context = empty_setup().await;

    // empty spend_query
    let resources_per_asset = context
        .client
        .resources_to_spend(format!("{owner:#x}").as_str(), vec![], None)
        .await
        .unwrap();
    assert!(resources_per_asset.is_empty());
}

#[rstest::rstest]
#[tokio::test]
async fn resources_to_spend_error_duplicate_asset_query(
    #[values(Address::default(), Address::from([5; 32]), Address::from([16; 32]))]
    owner: Address,
    #[values(AssetId::new([1u8; 32]), AssetId::new([99u8; 32]))] asset_id: AssetId,
) {
    let context = empty_setup().await;

    // the queries with the same id
    let resources_per_asset = context
        .client
        .resources_to_spend(
            format!("{owner:#x}").as_str(),
            vec![
                (format!("{asset_id:#x}").as_str(), 1, None),
                (format!("{asset_id:#x}").as_str(), 2, None),
                (format!("{asset_id:#x}").as_str(), 3, None),
            ],
            None,
        )
        .await;
    assert!(resources_per_asset.is_err());
    assert_eq!(
        resources_per_asset.unwrap_err().to_string(),
        ResourceQueryError::DuplicateAssets(asset_id).to_str_error_string()
    );
}

trait ToStdErrorString {
    fn to_str_error_string(self) -> String;
}

impl ToStdErrorString for ResourceQueryError {
    fn to_str_error_string(self) -> String {
        fuel_core_client::client::from_strings_errors_to_std_error(vec![self.to_string()])
            .to_string()
    }
}

trait CumulativeAmount {
    fn amount(&self) -> u64;
}

impl CumulativeAmount for Vec<Resource> {
    fn amount(&self) -> u64 {
        self.iter().map(|resource| resource.amount()).sum()
    }
}
