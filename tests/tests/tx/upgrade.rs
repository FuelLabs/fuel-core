use fuel_core_client::client::types::TransactionStatus;
use fuel_core_types::{
    fuel_tx::{
        AssetId,
        Bytes32,
        GasCosts,
        Input,
        Receipt,
        Transaction,
        UpgradePurpose,
        UploadSubsection,
        policies::Policies,
    },
    fuel_vm::UploadedBytecode,
};
use fuel_core_upgradable_executor::WASM_BYTECODE;
use itertools::Itertools;
use rand::{
    Rng,
    rngs::StdRng,
};
use test_helpers::{
    builder::{
        TestContext,
        TestSetupBuilder,
    },
    predicate,
    transactions_from_subsections,
    valid_input,
};

const SUBSECTION_SIZE: usize = 64 * 1024;

#[tokio::test]
async fn can_upload_current_state_transition_function() {
    let amount = 1_000;
    let subsections =
        UploadSubsection::split_bytecode(WASM_BYTECODE, SUBSECTION_SIZE).unwrap();

    // Given
    let mut test_builder = TestSetupBuilder::new(2322);
    let transactions =
        transactions_from_subsections(&mut test_builder.rng, subsections, amount);
    test_builder.config_coin_inputs_from_transactions(&transactions.iter().collect_vec());

    let TestContext {
        client, srv: _drop, ..
    } = test_builder.finalize().await;

    for upload in transactions {
        // When
        let tx = upload.into();
        let estimate_predicates = true;
        let result = client
            .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
            .await;

        // Then
        let result = result.expect("We should be able to upload the bytecode subsection");
        assert!(matches!(result, TransactionStatus::Success { .. }))
    }
}

#[tokio::test]
async fn can_upgrade_to_uploaded_state_transition() {
    let privileged_address = Input::predicate_owner(predicate());
    let amount = 1_000;
    let subsections =
        UploadSubsection::split_bytecode(WASM_BYTECODE, SUBSECTION_SIZE).unwrap();
    let root = subsections[0].root;
    let mut test_builder = TestSetupBuilder::new(2322);
    test_builder.utxo_validation = false;
    test_builder.privileged_address = privileged_address;
    let TestContext {
        client,
        srv: _drop,
        mut rng,
        ..
    } = test_builder.finalize().await;

    let transactions = transactions_from_subsections(&mut rng, subsections, amount);
    for upload in transactions {
        let tx = upload.into();
        let estimate_predicates = true;
        client
            .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
            .await
            .unwrap();
    }

    // Given
    let upgrade = Transaction::upgrade(
        UpgradePurpose::StateTransition { root },
        Policies::new().with_max_fee(amount),
        vec![Input::coin_predicate(
            rng.r#gen(),
            privileged_address,
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

    // When
    let tx = upgrade.into();
    let estimate_predicates = true;
    let result = client
        .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
        .await;

    // Then
    let TransactionStatus::Success { block_height, .. } =
        result.expect("We should be able to upgrade to the uploaded bytecode")
    else {
        unreachable!()
    };
    let state_transition_bytecode_version_before_upgrade = client
        .block_by_height(block_height)
        .await
        .unwrap()
        .unwrap()
        .header
        .state_transition_bytecode_version;
    let next_block = client.produce_blocks(1, None).await.unwrap();
    let state_transition_bytecode_version_after_upgrade = client
        .block_by_height(next_block)
        .await
        .unwrap()
        .unwrap()
        .header
        .state_transition_bytecode_version;
    assert_ne!(
        state_transition_bytecode_version_before_upgrade,
        state_transition_bytecode_version_after_upgrade
    );
}

#[tokio::test]
async fn upgrading_to_invalid_state_transition_fails() {
    let privileged_address = Input::predicate_owner(predicate());
    let amount = 1_000;
    let subsections = UploadSubsection::split_bytecode(
        b"This is definitely not valid wasm!",
        SUBSECTION_SIZE,
    )
    .unwrap();
    let root = subsections[0].root;
    let mut test_builder = TestSetupBuilder::new(2322);
    test_builder.utxo_validation = false;
    test_builder.privileged_address = privileged_address;
    let TestContext {
        client,
        srv: _drop,
        mut rng,
        ..
    } = test_builder.finalize().await;

    let transactions = transactions_from_subsections(&mut rng, subsections, amount);
    for upload in transactions {
        let tx = upload.into();
        let estimate_predicates = true;
        client
            .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
            .await
            .unwrap();
    }

    // Given
    let upgrade = Transaction::upgrade(
        UpgradePurpose::StateTransition { root },
        Policies::new().with_max_fee(amount),
        vec![Input::coin_predicate(
            rng.r#gen(),
            privileged_address,
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

    // When
    let tx = upgrade.into();
    let estimate_predicates = true;
    let result = client
        .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
        .await;

    // Then
    let result_str = format!("{:?}", result); // io::Result forces string handling
    result.expect_err("Upgrading to an incorrect bytecode should fail");
    assert!(
        result_str.contains("Error with Wasm validity: NotValid"),
        "msg: {}",
        result_str
    );
}

#[tokio::test]
async fn upgrading_to_missing_state_transition_fails() {
    let privileged_address = Input::predicate_owner(predicate());
    let amount = 1_000;
    let mut test_builder = TestSetupBuilder::new(2322);
    test_builder.utxo_validation = false;
    test_builder.privileged_address = privileged_address;
    let TestContext {
        client,
        srv: _drop,
        mut rng,
        ..
    } = test_builder.finalize().await;

    // Given
    let upgrade = Transaction::upgrade(
        UpgradePurpose::StateTransition {
            root: Bytes32::new([1; 32]),
        },
        Policies::new().with_max_fee(amount),
        vec![Input::coin_predicate(
            rng.r#gen(),
            privileged_address,
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

    // When
    let tx = upgrade.into();
    let estimate_predicates = true;
    let result = client
        .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
        .await;

    // Then
    let result_str = format!("{:?}", result); // io::Result forces string handling
    result.expect_err("Upgrading to missing bytecode should fail");
    assert!(
        result_str.contains("Error with Wasm validity: NotFound"),
        "msg: {}",
        result_str
    );
}

#[tokio::test]
async fn upgrade_to_a_partially_uploaded_state_transition_fails() {
    let privileged_address = Input::predicate_owner(predicate());
    let amount = 1_000;
    let subsections =
        UploadSubsection::split_bytecode(WASM_BYTECODE, SUBSECTION_SIZE).unwrap();
    let root = subsections[0].root;
    let mut test_builder = TestSetupBuilder::new(2322);
    test_builder.utxo_validation = false;
    test_builder.privileged_address = privileged_address;
    let TestContext {
        client,
        srv: _drop,
        mut rng,
        ..
    } = test_builder.finalize().await;

    let mut transactions = transactions_from_subsections(&mut rng, subsections, amount);
    assert!(transactions.len() > 1);
    let _ = transactions.pop(); // Don't upload the last subsection
    for upload in transactions {
        let tx = upload.into();
        let estimate_predicates = true;
        client
            .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
            .await
            .unwrap();
    }

    // Given
    let upgrade = Transaction::upgrade(
        UpgradePurpose::StateTransition { root },
        Policies::new().with_max_fee(amount),
        vec![Input::coin_predicate(
            rng.r#gen(),
            privileged_address,
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

    // When
    let tx = upgrade.into();
    let estimate_predicates = true;
    let result = client
        .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
        .await;

    // Then
    let result_str = format!("{:?}", result); // io::Result forces string handling
    result.expect_err("Upgrading to missing bytecode should fail");
    assert!(
        result_str.contains("Response errors; Error with Wasm validity: NotFound"),
        "msg: {}",
        result_str
    );
}

fn valid_transaction(rng: &mut StdRng, amount: u64) -> Transaction {
    Transaction::script(
        10_000,
        predicate(),
        vec![],
        Policies::new().with_max_fee(amount),
        vec![valid_input(rng, amount)],
        vec![],
        vec![],
    )
    .into()
}

fn used_gas(receipts: Vec<Receipt>) -> u64 {
    let mut used_gas = 0;
    for r in receipts {
        if let Receipt::ScriptResult { gas_used, .. } = r {
            used_gas = gas_used;
            break
        }
    }
    used_gas
}

#[tokio::test]
async fn upgrade_of_consensus_parameters_affects_used_gas_of_next_tx() {
    let privileged_address = Input::predicate_owner(predicate());
    let amount = 1_000;
    let mut test_builder = TestSetupBuilder::new(2322);
    test_builder.utxo_validation = false;
    test_builder.privileged_address = privileged_address;
    let TestContext {
        client,
        srv: _drop,
        mut rng,
        ..
    } = test_builder.finalize().await;

    // Given
    let tx = valid_transaction(&mut rng, amount);
    let estimate_predicates = true;
    let result = client
        .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
        .await
        .unwrap();
    let TransactionStatus::Success { receipts, .. } = result else {
        panic!("{result:?}")
    };
    let used_gas_before_upgrade = used_gas(receipts);

    // When
    let mut new_consensus_parameters =
        client.chain_info().await.unwrap().consensus_parameters;
    new_consensus_parameters.set_gas_costs(GasCosts::free());
    let upgrade = Transaction::upgrade_consensus_parameters(
        &new_consensus_parameters,
        Policies::new().with_max_fee(amount),
        vec![Input::coin_predicate(
            rng.r#gen(),
            privileged_address,
            amount,
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
    let tx = upgrade.into();
    let estimate_predicates = true;
    client
        .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
        .await
        .unwrap();

    // Then
    let tx = valid_transaction(&mut rng, amount);
    let estimate_predicates = true;
    let result = client
        .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
        .await
        .unwrap();
    let TransactionStatus::Success { receipts, .. } = result else {
        panic!("{result:?}")
    };
    let used_gas_after_upgrade = used_gas(receipts);
    assert_ne!(used_gas_before_upgrade, used_gas_after_upgrade);
}

#[tokio::test]
async fn old_consensus_parameters_should_be_queryable_after_upgrade() {
    let privileged_address = Input::predicate_owner(predicate());
    let amount = 1_000;
    let mut test_builder = TestSetupBuilder::new(2322);
    test_builder.utxo_validation = false;
    test_builder.privileged_address = privileged_address;
    let TestContext {
        client,
        srv: _drop,
        mut rng,
        ..
    } = test_builder.finalize().await;

    // Given
    let mut new_consensus_parameters =
        client.chain_info().await.unwrap().consensus_parameters;
    new_consensus_parameters.set_gas_costs(GasCosts::free());
    let upgrade = Transaction::upgrade_consensus_parameters(
        &new_consensus_parameters,
        Policies::new().with_max_fee(amount),
        vec![Input::coin_predicate(
            rng.r#gen(),
            privileged_address,
            amount,
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
    let tx = upgrade.into();
    let estimate_predicates = true;
    client
        .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
        .await
        .unwrap();
    let next_block_height = client.produce_blocks(1, None).await.unwrap();

    let latest_consensus_parameters_version = client
        .block_by_height(next_block_height)
        .await
        .unwrap()
        .expect("block doesn't exist")
        .header
        .consensus_parameters_version;

    let previous_consensus_parameters_version =
        latest_consensus_parameters_version.checked_sub(1).unwrap();

    let latest_consensus_parameters = client
        .consensus_parameters(latest_consensus_parameters_version as i32)
        .await
        .expect("consensus parameter query failed")
        .expect("missing new consensus parameters");

    let previous_consensus_parameters = client
        .consensus_parameters(previous_consensus_parameters_version as i32)
        .await
        .expect("consensus parameter query failed")
        .expect("missing old consensus parameters");

    // Then
    assert_eq!(previous_consensus_parameters_version, 0);
    assert_ne!(previous_consensus_parameters, latest_consensus_parameters);
}

#[tokio::test]
async fn state_transition_bytecode_should_be_queryable_by_its_root_and_version() {
    let privileged_address = Input::predicate_owner(predicate());
    let amount = 1_000;
    let subsections =
        UploadSubsection::split_bytecode(WASM_BYTECODE, SUBSECTION_SIZE).unwrap();
    let root = subsections[0].root;
    let mut test_builder = TestSetupBuilder::new(2322);
    test_builder.utxo_validation = false;
    test_builder.privileged_address = privileged_address;
    let TestContext {
        client,
        srv: _drop,
        mut rng,
        ..
    } = test_builder.finalize().await;

    let transactions = transactions_from_subsections(&mut rng, subsections, amount);
    for upload in transactions {
        let tx = upload.into();
        let estimate_predicates = true;
        client
            .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
            .await
            .unwrap();
    }

    // Given
    let upgrade = Transaction::upgrade(
        UpgradePurpose::StateTransition { root },
        Policies::new().with_max_fee(amount),
        vec![Input::coin_predicate(
            rng.r#gen(),
            privileged_address,
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

    // When
    let tx = upgrade.into();
    let estimate_predicates = true;
    client
        .submit_and_await_commit_opt(&tx, Some(estimate_predicates))
        .await
        .unwrap();

    let next_block_height = client.produce_blocks(1, None).await.unwrap();

    let latest_state_transition_bytecode_version = client
        .block_by_height(next_block_height)
        .await
        .unwrap()
        .expect("block doesn't exist")
        .header
        .state_transition_bytecode_version;

    let state_transition_bytecode_by_root = client
        .state_transition_byte_code_by_root(root)
        .await
        .unwrap()
        .expect("no bytecode");
    let state_transition_bytecode_by_version = client
        .state_transition_byte_code_by_version(
            latest_state_transition_bytecode_version as i32,
        )
        .await
        .unwrap()
        .expect("no bytecode");

    // Then
    assert_eq!(state_transition_bytecode_by_root.root, root);
    assert_eq!(
        state_transition_bytecode_by_version,
        state_transition_bytecode_by_root
    );
    match state_transition_bytecode_by_root.bytecode {
        UploadedBytecode::Completed(bytecode) => assert_eq!(bytecode, WASM_BYTECODE),
        _ => panic!("bytecode uploade incomplete"),
    };
}
