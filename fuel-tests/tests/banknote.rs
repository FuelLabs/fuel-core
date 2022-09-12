use fuel_core::{
    chain_config::{
        CoinConfig,
        StateConfig,
    },
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_interfaces::common::{
    fuel_tx::AssetId,
    fuel_vm::prelude::Address,
};
use fuel_gql_client::client::{
    schema::banknote::Banknote,
    FuelClient,
};

#[tokio::test]
async fn banknotes_to_spend() {
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

    // empty spend_query
    let banknotes_per_asset = client
        .banknotes_to_spend(format!("{:#x}", owner).as_str(), vec![], None, None)
        .await
        .unwrap();
    assert!(banknotes_per_asset.is_empty());

    // spend_query for 1 a and 1 b
    let banknotes_per_asset = client
        .banknotes_to_spend(
            format!("{:#x}", owner).as_str(),
            vec![
                (format!("{:#x}", asset_id_a).as_str(), 1),
                (format!("{:#x}", asset_id_b).as_str(), 1),
            ],
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(banknotes_per_asset.len(), 2);

    // spend_query for 300 a and 300 b
    let banknotes_per_asset = client
        .banknotes_to_spend(
            format!("{:#x}", owner).as_str(),
            vec![
                (format!("{:#x}", asset_id_a).as_str(), 300),
                (format!("{:#x}", asset_id_b).as_str(), 300),
            ],
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(banknotes_per_asset.len(), 2);
    assert_eq!(banknotes_per_asset[0].len(), 3);
    assert_eq!(banknotes_per_asset[1].len(), 3);

    // spend_query for 1 a and 1 b, but with all banknotes excluded
    let all_utxos: Vec<String> = banknotes_per_asset
        .iter()
        .map(|banknotes| {
            banknotes.into_iter().filter_map(|b| match b {
                Banknote::Coin(c) => Some(format!("{:#x}", c.utxo_id)),
                Banknote::Message(_) => None,
            })
        })
        .flatten()
        .collect();
    let all_banknote_ids = all_utxos.iter().map(String::as_str).collect();
    let banknotes_per_asset = client
        .banknotes_to_spend(
            format!("{:#x}", owner).as_str(),
            vec![
                (format!("{:#x}", asset_id_a).as_str(), 1),
                (format!("{:#x}", asset_id_b).as_str(), 1),
            ],
            None,
            Some((all_banknote_ids, vec![])),
        )
        .await;
    assert!(banknotes_per_asset.is_err());

    // not enough banknotes
    let banknotes_per_asset = client
        .banknotes_to_spend(
            format!("{:#x}", owner).as_str(),
            vec![
                (format!("{:#x}", asset_id_a).as_str(), 301),
                (format!("{:#x}", asset_id_b).as_str(), 301),
            ],
            None,
            None,
        )
        .await;
    assert!(banknotes_per_asset.is_err());

    // not enough inputs
    let banknotes_per_asset = client
        .banknotes_to_spend(
            format!("{:#x}", owner).as_str(),
            vec![
                (format!("{:#x}", asset_id_a).as_str(), 300),
                (format!("{:#x}", asset_id_b).as_str(), 300),
            ],
            2.into(),
            None,
        )
        .await;
    assert!(banknotes_per_asset.is_err());
}
