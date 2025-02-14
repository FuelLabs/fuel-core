use fuel_core::service::Config;
use fuel_core_bin::FuelService;
use fuel_core_client::client::FuelClient;
use fuel_core_types::fuel_tx::{
    policies::Policies,
    Address,
    AssetId,
    GasCosts,
    Input,
    Transaction,
};
use rand::{
    rngs::StdRng,
    Rng,
};
use serde_json::Value;
use test_helpers::{
    builder::{
        TestContext,
        TestSetupBuilder,
    },
    get_graphql_extension_field_value,
    predicate,
    send_graph_ql_query,
};

const QUERY: &str = r#"
    query {
        nodeInfo {
            nodeVersion
        }
    }
"#;

#[tokio::test]
async fn extension_fields_are_present() {
    const REQUIRED_FIELDS: [&str; 3] = [
        "current_stf_version",
        "current_fuel_block_height",
        "current_consensus_parameters_version",
    ];

    // Given
    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    // When
    let response = send_graph_ql_query(&url, QUERY).await;

    // Then
    let json_value: Value =
        serde_json::from_str(&response).expect("should be valid json");
    let extensions = json_value
        .get("extensions")
        .expect("should have extensions");
    for field in REQUIRED_FIELDS.iter() {
        let is_field_present = extensions.get(field).is_some();
        assert!(is_field_present)
    }
}

async fn upgrade_consensus_parameters(
    rng: &mut StdRng,
    client: &FuelClient,
    privileged_address: &Address,
) {
    const AMOUNT: u64 = 1_000;

    let mut new_consensus_parameters =
        client.chain_info().await.unwrap().consensus_parameters;
    new_consensus_parameters.set_gas_costs(GasCosts::free());

    let upgrade = Transaction::upgrade_consensus_parameters(
        &new_consensus_parameters,
        Policies::new().with_max_fee(AMOUNT),
        vec![Input::coin_predicate(
            rng.gen(),
            *privileged_address,
            AMOUNT,
            AssetId::BASE,
            Default::default(),
            Default::default(),
            predicate(),
            vec![],
        )],
        vec![],
        vec![],
    )
    .unwrap();

    // When
    let mut tx = upgrade.into();
    client.estimate_predicates(&mut tx).await.unwrap();
    client.submit_and_await_commit(&tx).await.unwrap();
    client.produce_blocks(1, None).await.unwrap();
}

#[tokio::test]
async fn graphql_extensions_should_provide_new_consensus_parameters_version_after_upgrade(
) {
    const EXTENSION_FIELD: &str = "current_consensus_parameters_version";

    let mut test_builder = TestSetupBuilder::new(2322);
    let privileged_address = Input::predicate_owner(predicate());
    test_builder.utxo_validation = false;
    test_builder.privileged_address = privileged_address;
    let TestContext {
        client,
        srv,
        mut rng,
        ..
    } = test_builder.finalize().await;
    let url = format!("http://{}/v1/graphql", srv.bound_address);

    // Given
    let pre_upgrade_version = get_graphql_extension_field_value(
        &send_graph_ql_query(&url, QUERY).await,
        EXTENSION_FIELD,
    );

    // When
    upgrade_consensus_parameters(&mut rng, &client, &privileged_address).await;

    // Then
    let post_upgrade_version = get_graphql_extension_field_value(
        &send_graph_ql_query(&url, QUERY).await,
        EXTENSION_FIELD,
    );

    assert_eq!(post_upgrade_version, pre_upgrade_version + 1);
}
