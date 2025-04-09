#![allow(non_snake_case)]

use clap::Parser;
use fuel_core::{
    chain_config::TESTNET_WALLET_SECRETS,
    combined_database::CombinedDatabase,
    schema::tx::types::TransactionStatus,
    service::{
        config::fuel_core_importer::ports::Validator,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::TransactionStatus as ClientTransactionStatus,
    FuelClient,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    fuel_tx::{
        AssetId,
        Input,
        Output,
        Receipt,
        Transaction,
        TransactionBuilder,
        TxId,
        UniqueIdentifier,
    },
    fuel_types::BlockHeight,
    fuel_vm::SecretKey,
    secrecy::Secret,
    services::executor::TransactionExecutionResult,
    signer::SignMode,
};
use futures::StreamExt;
use itertools::Itertools;
use rand::{
    prelude::StdRng,
    Rng,
    SeedableRng,
};
use std::{
    collections::BTreeSet,
    str::FromStr,
    vec,
};
use tempfile::TempDir;
use test_helpers::{
    assemble_tx::{
        AssembleAndRunTx,
        SigningAccount,
    },
    config_with_fee,
    counter_contract,
    fuel_core_driver::FuelCoreDriver,
    produce_block_with_tx,
};

fn transfer_transaction(min_amount: u64, rng: &mut StdRng) -> Transaction {
    let mut builder = TransactionBuilder::script(vec![], vec![]);

    let number_of_inputs = rng.gen_range(1..10);

    for _ in 0..number_of_inputs {
        let utxo_id = rng.gen();
        let owner: [u8; 32] = [rng.gen_range(0..10); 32];
        let amount = rng.gen_range(min_amount..min_amount + 100_000);
        builder.add_input(Input::coin_predicate(
            utxo_id,
            owner.into(),
            amount,
            AssetId::BASE,
            Default::default(),
            0,
            vec![0],
            vec![],
        ));
    }

    for _ in 0..number_of_inputs {
        let owner: [u8; 32] = [rng.gen_range(0..10); 32];
        builder.add_output(Output::coin(owner.into(), min_amount, AssetId::BASE));
    }

    builder.finalize_as_transaction()
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_block_at_any_height__only_transfers() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let driver = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
        "--state-rewind-duration",
        "7d",
    ])
    .await?;
    let node = &driver.node;

    // Given
    const TOTAL_BLOCKS: u64 = 1000;
    const MIN_AMOUNT: u64 = 123456;
    let mut last_block_height = 0u32;
    let mut database_modifications = std::collections::HashMap::new();
    for _ in 0..TOTAL_BLOCKS {
        let mut blocks = node.shared.block_importer.events();
        let tx = transfer_transaction(MIN_AMOUNT, &mut rng);
        let result = node.submit_and_await_commit(tx).await.unwrap();
        assert!(matches!(result, TransactionStatus::Success(_)));

        let block = blocks.next().await.unwrap();
        let block_height = *block.shared_result.sealed_block.entity.header().height();
        last_block_height = block_height.into();
        database_modifications.insert(last_block_height, block.changes.as_ref().clone());
    }

    for i in 0..TOTAL_BLOCKS {
        let height_to_execute = rng.gen_range(1..last_block_height);

        let view = node.shared.database.on_chain().latest_view().unwrap();
        let block = view
            .get_full_block(&height_to_execute.into())
            .unwrap()
            .unwrap();

        // When
        tracing::info!("Validating block {i} at height {}", height_to_execute);
        let result = node.shared.executor.validate(&block);

        // Then
        let height_to_execute: BlockHeight = height_to_execute.into();
        let result = result.unwrap();
        let expected_changes = database_modifications.get(&height_to_execute).unwrap();
        let actual_changes = result.into_changes();
        assert_eq!(&actual_changes, expected_changes);
    }

    driver.kill().await;
    Ok(())
}

fn all_real_transactions(node: &FuelService) -> BTreeSet<TxId> {
    node.shared
        .database
        .on_chain()
        .all_transactions(None, None)
        .filter_map(|tx| match tx {
            Ok(tx) => {
                if tx.is_mint() {
                    None
                } else {
                    Some(Ok(tx.id(&Default::default())))
                }
            }
            Err(err) => Some(Err(err)),
        })
        .try_collect()
        .unwrap()
}

async fn rollback_existing_chain_to_target_height_and_verify(
    target_height: u32,
    blocks_in_the_chain: u32,
) -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let driver = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
        "--state-rewind-duration",
        "7d",
    ])
    .await?;
    let node = &driver.node;

    // Given
    const MIN_AMOUNT: u64 = 123456;
    let mut transactions = vec![];
    for _ in 0..blocks_in_the_chain {
        let tx = transfer_transaction(MIN_AMOUNT, &mut rng);
        transactions.push(tx.id(&Default::default()));

        let result = node.submit_and_await_commit(tx).await.unwrap();
        assert!(
            matches!(result, TransactionStatus::Success(_)),
            "Transaction got unexpected status {:?}",
            result
        );
    }
    let all_transactions = all_real_transactions(node);
    assert_eq!(all_transactions.len(), blocks_in_the_chain as usize);

    // When
    let temp_dir = driver.kill().await;
    let target_block_height = target_height.to_string();
    let args = [
        "_IGNORED_",
        "--db-path",
        temp_dir.path().to_str().unwrap(),
        "--target-block-height",
        target_block_height.as_str(),
    ];
    let command = fuel_core_bin::cli::rollback::Command::parse_from(args);
    tracing::info!("Rolling back to block {}", target_block_height);
    fuel_core_bin::cli::rollback::exec(command).await?;

    // Then
    let driver = FuelCoreDriver::spawn_feeless_with_directory(
        temp_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--state-rewind-duration",
            "7d",
        ],
    )
    .await?;
    let node = &driver.node;
    let remaining_transactions = all_real_transactions(node);

    let expected_transactions = transactions
        .into_iter()
        .take(target_height as usize)
        .collect::<BTreeSet<_>>();
    assert_eq!(remaining_transactions.len(), expected_transactions.len());
    pretty_assertions::assert_eq!(remaining_transactions, expected_transactions);

    let latest_height = node
        .shared
        .database
        .on_chain()
        .latest_height_from_metadata();
    assert_eq!(Ok(Some(BlockHeight::new(target_height))), latest_height);

    driver.kill().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn rollback_chain_to_genesis() -> anyhow::Result<()> {
    let genesis_block_height = 0;
    let blocks_in_the_chain = 100;
    rollback_existing_chain_to_target_height_and_verify(
        genesis_block_height,
        blocks_in_the_chain,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn rollback_chain_to_middle() -> anyhow::Result<()> {
    let target_rollback_block_height = 50;
    let blocks_in_the_chain = 100;
    rollback_existing_chain_to_target_height_and_verify(
        target_rollback_block_height,
        blocks_in_the_chain,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn rollback_chain_to_same_height() -> anyhow::Result<()> {
    let target_rollback_block_height = 100;
    let blocks_in_the_chain = 100;
    rollback_existing_chain_to_target_height_and_verify(
        target_rollback_block_height,
        blocks_in_the_chain,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn rollback_chain_to_same_height_1000() -> anyhow::Result<()> {
    let target_rollback_block_height = 800;
    let blocks_in_the_chain = 1000;
    rollback_existing_chain_to_target_height_and_verify(
        target_rollback_block_height,
        blocks_in_the_chain,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn rollback_to__should_work_with_empty_gas_price_database() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let driver = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
        "--state-rewind-duration",
        "7d",
    ])
    .await?;

    // Given
    const TOTAL_BLOCKS: u64 = 500;
    for _ in 0..TOTAL_BLOCKS {
        produce_block_with_tx(&mut rng, &driver.client).await;
    }
    let temp_dir = driver.kill().await;
    std::fs::remove_dir_all(temp_dir.path().join("gas_price")).unwrap();

    // When
    let args = [
        "_IGNORED_",
        "--db-path",
        temp_dir.path().to_str().unwrap(),
        "--target-block-height",
        "1",
    ];
    let command = fuel_core_bin::cli::rollback::Command::parse_from(args);
    let result = fuel_core_bin::cli::rollback::exec(command).await;

    // Then
    result.expect("Rollback should succeed");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn backup_and_restore__should_work_with_state_rewind() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let driver = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
        "--state-rewind-duration",
        "7d",
    ])
    .await?;
    let node = &driver.node;

    // Given
    // setup a node, produce some blocks
    const TOTAL_BLOCKS: u64 = 20;
    const MIN_AMOUNT: u64 = 123456;
    let mut last_block_height = 0u32;
    let mut database_modifications = std::collections::HashMap::new();
    for _ in 0..TOTAL_BLOCKS {
        let mut blocks = node.shared.block_importer.events();
        let tx = transfer_transaction(MIN_AMOUNT, &mut rng);
        let result = node.submit_and_await_commit(tx).await.unwrap();
        assert!(matches!(result, TransactionStatus::Success(_)));

        let block = blocks.next().await.unwrap();
        let block_height = *block.shared_result.sealed_block.entity.header().height();
        last_block_height = block_height.into();
        database_modifications.insert(last_block_height, block.changes.as_ref().clone());
    }

    // create a backup and delete the database
    let db_dir = driver.kill().await;
    let backup_dir = TempDir::new().unwrap();
    CombinedDatabase::backup(db_dir.path(), backup_dir.path()).unwrap();
    drop(db_dir);

    // restore the backup
    let new_db_dir = TempDir::new().unwrap();
    CombinedDatabase::restore(new_db_dir.path(), backup_dir.path()).unwrap();

    // start the node again, with the new db
    let driver = FuelCoreDriver::spawn_feeless_with_directory(
        new_db_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--state-rewind-duration",
            "7d",
        ],
    )
    .await
    .unwrap();
    let node = &driver.node;

    for i in 0..TOTAL_BLOCKS {
        let height_to_execute = rng.gen_range(1..last_block_height);

        let view = node.shared.database.on_chain().latest_view().unwrap();
        let block = view
            .get_full_block(&height_to_execute.into())
            .unwrap()
            .unwrap();

        // When
        tracing::info!("Validating block {i} at height {}", height_to_execute);
        let result = node.shared.executor.validate(&block);

        // Then
        let height_to_execute: BlockHeight = height_to_execute.into();
        let result = result.unwrap();
        let expected_changes = database_modifications.get(&height_to_execute).unwrap();
        let actual_changes = result.into_changes();
        assert_eq!(&actual_changes, expected_changes);
    }

    driver.kill().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run__correct_utxoid_state_in_past_blocks() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let poa_secret = SecretKey::random(&mut rng);

    let mut config = config_with_fee();
    config.consensus_signer = SignMode::Key(Secret::new(poa_secret.into()));
    let base_asset_id = config.base_asset_id();

    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // First, distribute one test wallet to multiple addresses
    let wallet_secret =
        SecretKey::from_str(TESTNET_WALLET_SECRETS[1]).expect("Expected valid secret");

    let wallets: Vec<SecretKey> = (0..2).map(|_| SecretKey::random(&mut rng)).collect();

    let recipients = wallets
        .iter()
        .map(|secret_key| {
            let address = SigningAccount::Wallet(*secret_key).owner();
            (address, base_asset_id, 1_000_000_000)
        })
        .collect();
    let status = client
        .run_transfer(SigningAccount::Wallet(wallet_secret), recipients)
        .await
        .unwrap();
    let ClientTransactionStatus::Success {
        block_height: bh_first,
        ..
    } = status
    else {
        panic!("unexpected result {status:?}")
    };

    // Then, transfer one of these coins to another wallet
    let source_wallet = wallets[0];
    let target_wallet = wallets[1];
    let target_address = SigningAccount::Wallet(target_wallet).owner();

    let second_tx = client
        .assemble_transfer(
            SigningAccount::Wallet(source_wallet),
            vec![(target_address, base_asset_id, 2)],
        )
        .await
        .unwrap();
    // `third_tx` is similar to `second_tx`, but sends less coins(`1` instead of `2`)
    // We want `third_tx` have a different TxId than `second_tx`.
    // Both transactions use the same UtxoId
    let third_tx = client
        .assemble_transfer(
            SigningAccount::Wallet(source_wallet),
            vec![(target_address, base_asset_id, 1)],
        )
        .await
        .unwrap();
    assert_eq!(second_tx.inputs(), third_tx.inputs());
    let status = client.submit_and_await_commit(&second_tx).await.unwrap();
    let ClientTransactionStatus::Success {
        block_height: bh_second,
        ..
    } = status
    else {
        panic!("unexpected result {status:?}")
    };

    // Then attempt to dry run the transfer again at different heights

    // At the first block, the coin doesn't exist, so it should fail
    let err = client
        .dry_run_opt(&[second_tx.clone()], None, None, Some(bh_first))
        .await
        .expect_err("should fail")
        .to_string();
    assert!(err.contains("The specified coin") && err.contains("doesn't exist"));

    // Just before the second transfer, the coin should exist and dry-run should succeed
    let status = client
        .dry_run_opt(&[second_tx.clone()], None, None, Some(bh_second))
        .await
        .expect("should succeed")[0]
        .clone();
    let TransactionExecutionResult::Success { .. } = status.result else {
        panic!("unexpected result {status:?}")
    };

    // At latest height, attempting to dry-run the same transaction again should fail
    let err = client
        .dry_run_opt(&[second_tx.clone()], None, None, None)
        .await
        .expect_err("should fail")
        .to_string();
    assert!(err.contains("Transaction id was already used"));

    // At latest height, a similar transaction with a different TxId should still fail
    let err = client
        .dry_run_opt(&[third_tx.clone()], None, None, None)
        .await
        .expect_err("should fail")
        .to_string();
    assert!(err.contains("The specified coin") && err.contains("doesn't exist"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run__correct_contract_state_in_past_blocks() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let driver = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--historical-execution",
        "--poa-instant",
        "true",
        "--state-rewind-duration",
        "7d",
    ])
    .await?;

    let (deployed_height, contract_id) =
        counter_contract::deploy(&driver.client, &mut rng).await;

    // Make one unrelated block
    produce_block_with_tx(&mut rng, &driver.client).await;
    let after_deploy_height = deployed_height.succ().unwrap();

    let (incr1_height, value1) =
        counter_contract::increment(&driver.client, &mut rng, contract_id).await;
    let (incr2_height, value2) =
        counter_contract::increment(&driver.client, &mut rng, contract_id).await;

    // Check that the contract works as expected when not dry-running
    assert!(incr1_height < incr2_height);
    assert_eq!(value1, 1);
    assert_eq!(value2, 2);

    // Dry-run at multiple points and check that the correct state is used
    for (block_height, expected_value) in [
        (Some(after_deploy_height), 1),
        (Some(incr1_height), 1),
        (Some(incr2_height), 2),
        (None, 3),
    ] {
        let res = driver
            .client
            .dry_run_opt(
                &[counter_contract::increment_tx(&mut rng, contract_id)],
                None,
                None,
                block_height,
            )
            .await?[0]
            .result
            .clone();

        let TransactionExecutionResult::Success { receipts, .. } = res else {
            panic!("Expected a successful execution");
        };
        assert!(receipts.len() > 2);
        let Receipt::Return { val, .. } = receipts[receipts.len() - 2] else {
            panic!("Expected a return receipt: {:?}", receipts);
        };
        assert_eq!(val, expected_value);
    }

    // Dry run just before the contract has been deployed to see that it fails
    driver
        .client
        .dry_run_opt(
            &[counter_contract::increment_tx(&mut rng, contract_id)],
            None,
            None,
            Some(deployed_height),
        )
        .await
        .expect_err("Should fail to dry-run at this height");

    driver.kill().await;
    Ok(())
}
