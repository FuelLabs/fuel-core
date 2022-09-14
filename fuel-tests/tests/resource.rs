use fuel_core::{
    chain_config::{
        ChainConfig,
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
use fuel_core_interfaces::{
    common::{
        fuel_tx::AssetId,
        fuel_vm::prelude::Address,
    },
    model::{
        BlockHeight,
        DaBlockHeight,
    },
};
use fuel_gql_client::client::{
    schema::resource::Resource,
    FuelClient,
};

mod coins {
    use super::*;

    async fn setup() -> (Address, AssetId, AssetId, FuelClient) {
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
            messages: None,
        });

        // setup server & client
        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        (owner, asset_id_a, asset_id_b, client)
    }

    #[tokio::test]
    async fn resources_to_spend_coins_query_target_1() {
        let (owner, asset_id_a, asset_id_b, client) = setup().await;

        // spend_query for 1 a and 1 b
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![
                    (format!("{:#x}", asset_id_a).as_str(), 1, None),
                    (format!("{:#x}", asset_id_b).as_str(), 1, None),
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

    #[tokio::test]
    async fn resources_to_spend_coins_query_target_300() {
        let (owner, asset_id_a, asset_id_b, client) = setup().await;

        // spend_query for 300 a and 300 b
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![
                    (format!("{:#x}", asset_id_a).as_str(), 300, None),
                    (format!("{:#x}", asset_id_b).as_str(), 300, None),
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

    #[tokio::test]
    async fn resources_to_spend_messages_exclude_all() {
        let (owner, asset_id_a, asset_id_b, client) = setup().await;

        // query all resources
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![
                    (format!("{:#x}", asset_id_a).as_str(), 300, None),
                    (format!("{:#x}", asset_id_b).as_str(), 300, None),
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
                })
            })
            .collect();
        let all_utxo_ids = all_utxos.iter().map(String::as_str).collect();
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![
                    (format!("{:#x}", asset_id_a).as_str(), 1, None),
                    (format!("{:#x}", asset_id_b).as_str(), 1, None),
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

    #[tokio::test]
    async fn resources_to_spend_coins_query_more_than_we_have() {
        let (owner, asset_id_a, asset_id_b, client) = setup().await;

        // not enough resources
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![
                    (format!("{:#x}", asset_id_a).as_str(), 301, None),
                    (format!("{:#x}", asset_id_b).as_str(), 301, None),
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

    #[tokio::test]
    async fn resources_to_spend_coins_query_limit_resources() {
        let (owner, asset_id_a, asset_id_b, client) = setup().await;

        // not enough inputs
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![
                    (format!("{:#x}", asset_id_a).as_str(), 300, Some(2)),
                    (format!("{:#x}", asset_id_b).as_str(), 300, Some(2)),
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
    use super::*;

    async fn setup() -> (Address, AssetId, FuelClient) {
        let owner = Address::default();
        let base_asset_id = ChainConfig::BASE_ASSET;

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
                        da_height: DaBlockHeight::from(BlockHeight::from(1u64)),
                    })
                    .collect(),
            ),
        });

        // setup server & client
        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        (owner, base_asset_id, client)
    }

    #[tokio::test]
    async fn resources_to_spend_coins_query_target_1() {
        let (owner, base_asset_id, client) = setup().await;

        // query resources for `base_asset_id` and target 1
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![(format!("{:#x}", base_asset_id).as_str(), 1, None)],
                None,
            )
            .await
            .unwrap();
        assert_eq!(resources_per_asset.len(), 1);
    }

    #[tokio::test]
    async fn resources_to_spend_coins_query_target_300() {
        let (owner, base_asset_id, client) = setup().await;

        // query for 300 base assets
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![(format!("{:#x}", base_asset_id).as_str(), 300, None)],
                None,
            )
            .await
            .unwrap();
        assert_eq!(resources_per_asset.len(), 1);
        assert_eq!(resources_per_asset[0].len(), 3);
    }

    #[tokio::test]
    async fn resources_to_spend_messages_exclude_all() {
        let (owner, base_asset_id, client) = setup().await;

        // query for 300 base assets
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![(format!("{:#x}", base_asset_id).as_str(), 300, None)],
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
                })
            })
            .collect();
        let all_message_ids = all_message_ids.iter().map(String::as_str).collect();
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![(format!("{:#x}", base_asset_id).as_str(), 1, None)],
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

    #[tokio::test]
    async fn resources_to_spend_coins_query_more_than_we_have() {
        let (owner, base_asset_id, client) = setup().await;

        // max resources reached
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![(format!("{:#x}", base_asset_id).as_str(), 301, None)],
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

    #[tokio::test]
    async fn resources_to_spend_coins_query_limit_resources() {
        let (owner, base_asset_id, client) = setup().await;

        // not enough inputs
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![(format!("{:#x}", base_asset_id).as_str(), 300, Some(2))],
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
    use super::*;

    async fn setup() -> (Address, AssetId, AssetId, FuelClient) {
        let owner = Address::default();
        let asset_id_a = ChainConfig::BASE_ASSET;
        let asset_id_b = AssetId::new([1u8; 32]);

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
                        da_height: DaBlockHeight::from(BlockHeight::from(1u64)),
                    })
                    .collect(),
            ),
        });

        // setup server & client
        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        (owner, asset_id_a, asset_id_b, client)
    }

    #[tokio::test]
    async fn resources_to_spend_coins_query_target_1() {
        let (owner, asset_id_a, asset_id_b, client) = setup().await;

        // query resources for `base_asset_id` and target 1
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![
                    (format!("{:#x}", asset_id_a).as_str(), 1, None),
                    (format!("{:#x}", asset_id_b).as_str(), 1, None),
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

    #[tokio::test]
    async fn resources_to_spend_coins_query_target_300() {
        let (owner, asset_id_a, asset_id_b, client) = setup().await;

        // query for 300 base assets
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![
                    (format!("{:#x}", asset_id_a).as_str(), 300, None),
                    (format!("{:#x}", asset_id_b).as_str(), 300, None),
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

    #[tokio::test]
    async fn resources_to_spend_messages_exclude_all() {
        let (owner, asset_id_a, asset_id_b, client) = setup().await;

        // query for 300 base assets
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![
                    (format!("{:#x}", asset_id_a).as_str(), 300, None),
                    (format!("{:#x}", asset_id_b).as_str(), 300, None),
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
                })
            })
            .collect();
        let all_utxo_ids: Vec<String> = resources_per_asset
            .iter()
            .flat_map(|resources| {
                resources.iter().filter_map(|b| match b {
                    Resource::Coin(c) => Some(format!("{:#x}", c.utxo_id)),
                    Resource::Message(_) => None,
                })
            })
            .collect();

        let all_message_ids: Vec<_> =
            all_message_ids.iter().map(String::as_str).collect();
        let all_utxo_ids: Vec<_> = all_utxo_ids.iter().map(String::as_str).collect();

        // After setup we have 4 `Coin`s and 2 `Message`s
        assert_eq!(all_utxo_ids.len(), 4);
        assert_eq!(all_message_ids.len(), 2);

        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![
                    (format!("{:#x}", asset_id_a).as_str(), 1, None),
                    (format!("{:#x}", asset_id_b).as_str(), 1, None),
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

    #[tokio::test]
    async fn resources_to_spend_coins_query_more_than_we_have() {
        let (owner, asset_id_a, asset_id_b, client) = setup().await;

        // max resources reached
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![
                    (format!("{:#x}", asset_id_a).as_str(), 301, None),
                    (format!("{:#x}", asset_id_b).as_str(), 301, None),
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

    #[tokio::test]
    async fn resources_to_spend_coins_query_limit_resources() {
        let (owner, asset_id_a, asset_id_b, client) = setup().await;

        // not enough inputs
        let resources_per_asset = client
            .resources_to_spend(
                format!("{:#x}", owner).as_str(),
                vec![
                    (format!("{:#x}", asset_id_a).as_str(), 300, Some(2)),
                    (format!("{:#x}", asset_id_b).as_str(), 300, Some(2)),
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

async fn empty_setup() -> (Address, FuelClient) {
    let owner = Address::default();

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

    (owner, client)
}

#[tokio::test]
async fn resources_to_spend_empty() {
    let (owner, client) = empty_setup().await;

    // empty spend_query
    let resources_per_asset = client
        .resources_to_spend(format!("{:#x}", owner).as_str(), vec![], None)
        .await
        .unwrap();
    assert!(resources_per_asset.is_empty());
}

#[tokio::test]
async fn resources_to_spend_error_duplicate_asset_query() {
    let (owner, client) = empty_setup().await;
    let asset_id = AssetId::new([1u8; 32]);

    // the queries with the same id
    let resources_per_asset = client
        .resources_to_spend(
            format!("{:#x}", owner).as_str(),
            vec![
                (format!("{:#x}", asset_id).as_str(), 1, None),
                (format!("{:#x}", asset_id).as_str(), 2, None),
                (format!("{:#x}", asset_id).as_str(), 3, None),
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
        fuel_gql_client::client::from_strings_errors_to_std_error(vec![self.to_string()])
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
