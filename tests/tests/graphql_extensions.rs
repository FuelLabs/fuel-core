use fuel_core::service::Config;
use fuel_core_bin::FuelService;
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_types::fuel_tx::{
    policies::Policies,
    Address,
    AssetId,
    Bytes32,
    GasCosts,
    Input,
    Transaction,
    UpgradePurpose,
    Upload,
    UploadSubsection,
};
use fuel_core_upgradable_executor::WASM_BYTECODE;
use itertools::Itertools;
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
    predicate,
    send_graph_ql_query,
    transactions_from_subsections,
};

#[tokio::test]
async fn extension_fields_are_present() {
    const QUERY: &str = r#"
        query {
            nodeInfo {
                nodeVersion
            }
        }
    "#;

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

    let mut tx = upgrade.into();
    client.estimate_predicates(&mut tx).await.unwrap();
    client.submit_and_await_commit(&tx).await.unwrap();
    client
        .produce_blocks(1, None)
        .await
        .expect("should produce block");
}

#[tokio::test]
async fn graphql_extensions_should_provide_new_consensus_parameters_version_after_upgrade(
) {
    let mut test_builder = TestSetupBuilder::new(2322);
    let privileged_address = Input::predicate_owner(predicate());
    test_builder.utxo_validation = false;
    test_builder.privileged_address = privileged_address;
    let TestContext {
        client,
        srv: _srv,
        mut rng,
        ..
    } = test_builder.finalize().await;
    client
        .produce_blocks(1, None)
        .await
        .expect("should produce block");

    // Given
    let pre_upgrade_version = client
        .latest_consensus_parameters_version()
        .expect("should have consensus parameters version");

    // When
    upgrade_consensus_parameters(&mut rng, &client, &privileged_address).await;

    // Then
    let post_upgrade_version = client
        .latest_consensus_parameters_version()
        .expect("should have consensus parameters version");

    assert_eq!(post_upgrade_version, pre_upgrade_version + 1);
}

fn prepare_upload_transactions(rng: &mut StdRng, amount: u64) -> (Bytes32, Vec<Upload>) {
    const SUBSECTION_SIZE: usize = 64 * 1024;

    let subsections =
        UploadSubsection::split_bytecode(WASM_BYTECODE, SUBSECTION_SIZE).unwrap();
    let root = subsections[0].root;

    let transactions = transactions_from_subsections(rng, subsections, amount);
    (root, transactions)
}

async fn upgrade_stf(
    rng: &mut StdRng,
    client: &FuelClient,
    privileged_address: &Address,
    transactions: impl Iterator<Item = Upload>,
    amount: u64,
    root: Bytes32,
) {
    for upload in transactions {
        let mut tx = upload.into();
        client
            .estimate_predicates(&mut tx)
            .await
            .expect("Should estimate transaction");
        let result = client.submit_and_await_commit(&tx).await;
        let result = result.expect("We should be able to upload the bytecode subsection");
        assert!(matches!(result, TransactionStatus::Success { .. }))
    }

    let upgrade = Transaction::upgrade(
        UpgradePurpose::StateTransition { root },
        Policies::new().with_max_fee(amount),
        vec![Input::coin_predicate(
            rng.gen(),
            *privileged_address,
            amount,
            AssetId::BASE,
            Default::default(),
            Default::default(),
            predicate(),
            vec![],
        )],
        vec![],
        vec![],
    );
    let mut tx = upgrade.into();
    client.estimate_predicates(&mut tx).await.unwrap();
    let result = client.submit_and_await_commit(&tx).await;
    let result = result.expect("We should be able to upgrade to the uploaded bytecode");
    assert!(matches!(result, TransactionStatus::Success { .. }));
    client
        .produce_blocks(1, None)
        .await
        .expect("should produce block");
}

#[tokio::test]
async fn graphql_extensions_should_provide_new_stf_version_after_upgrade() {
    const AMOUNT: u64 = 1_000;

    let mut test_builder = TestSetupBuilder::new(2322);
    let privileged_address = Input::predicate_owner(predicate());
    test_builder.utxo_validation = false;
    test_builder.privileged_address = privileged_address;
    let TestContext {
        client,
        srv: _srv,
        mut rng,
        ..
    } = test_builder.finalize().await;
    client
        .produce_blocks(1, None)
        .await
        .expect("should produce block");

    // Given
    let (root, transactions) = prepare_upload_transactions(&mut rng, AMOUNT);
    test_builder.config_coin_inputs_from_transactions(&transactions.iter().collect_vec());

    let pre_upgrade_version = client
        .latest_stf_version()
        .expect("should have stf version");

    // When
    upgrade_stf(
        &mut rng,
        &client,
        &privileged_address,
        transactions.into_iter(),
        AMOUNT,
        root,
    )
    .await;

    // Then
    let post_upgrade_version = client
        .latest_stf_version()
        .expect("should have stf version");
    assert_eq!(post_upgrade_version, pre_upgrade_version + 1);
}
