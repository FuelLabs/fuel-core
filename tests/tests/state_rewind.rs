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
    pagination::PaginationRequest,
    types::{
        Coin,
        TransactionStatus as ClientTransactionStatus,
    },
    FuelClient,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    fuel_tx::{
        Address,
        AssetId,
        GasCosts,
        Input,
        Output,
        Receipt,
        Transaction,
        TransactionBuilder,
        TxId,
        TxPointer,
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

    let view = node.shared.database.on_chain().latest_view().unwrap();
    for i in 0..TOTAL_BLOCKS {
        let height_to_execute = rng.gen_range(1..last_block_height);

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

    let view = node.shared.database.on_chain().latest_view().unwrap();

    for i in 0..TOTAL_BLOCKS {
        let height_to_execute = rng.gen_range(1..last_block_height);

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

/// Get arbitrary coin from a wallet
async fn get_wallet_coin(client: &FuelClient, wallet_address: &Address) -> Coin {
    let coins = client
        .coins(
            wallet_address,
            None,
            PaginationRequest {
                cursor: None,
                results: 10,
                direction: fuel_core_client::client::pagination::PageDirection::Forward,
            },
        )
        .await
        .expect("Unable to get coins")
        .results;
    coins
        .into_iter()
        .next()
        .expect("Expected at least one coin")
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run__correct_utxoid_state_in_past_blocks() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let poa_secret = SecretKey::random(&mut rng);

    let mut config = fuel_core::service::Config::local_node();
    config.consensus_signer = SignMode::Key(Secret::new(poa_secret.into()));
    config.utxo_validation = true;
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // First, distribute one test wallet to multiple addresses
    let wallet_secret =
        SecretKey::from_str(TESTNET_WALLET_SECRETS[1]).expect("Expected valid secret");
    let wallet_address = Address::from(*wallet_secret.public_key().hash());
    let coin = get_wallet_coin(&client, &wallet_address).await;

    let wallets: Vec<SecretKey> = (0..2).map(|_| SecretKey::random(&mut rng)).collect();

    let mut tx = TransactionBuilder::script(vec![], vec![]);
    tx.max_fee_limit(0)
        .script_gas_limit(1_000_000)
        .with_gas_costs(GasCosts::free())
        .add_unsigned_coin_input(
            wallet_secret,
            coin.utxo_id,
            coin.amount,
            coin.asset_id,
            TxPointer::new(coin.block_created.into(), coin.tx_created_idx),
        );
    for key in wallets.iter() {
        let public = key.public_key();
        let owner: [u8; Address::LEN] = public.hash().into();

        tx.add_output(Output::Coin {
            to: owner.into(),
            amount: coin.amount / (wallets.len() as u64),
            asset_id: coin.asset_id,
        });
    }
    let first_tx = tx.finalize_as_transaction();
    let status = client.submit_and_await_commit(&first_tx).await.unwrap();
    let ClientTransactionStatus::Success {
        block_height: bh_first,
        ..
    } = status
    else {
        panic!("unexpected result {status:?}")
    };

    // Then, transfer one of these coins to another wallet
    let source_public = wallets[0].public_key();
    let source_owner: [u8; Address::LEN] = source_public.hash().into();
    let source_coin = get_wallet_coin(&client, &source_owner.into()).await;
    let target_public = wallets[1].public_key();
    let target_owner: [u8; Address::LEN] = target_public.hash().into();
    let second_tx = TransactionBuilder::script(vec![], vec![])
        .max_fee_limit(0)
        .script_gas_limit(1_000_000)
        .with_gas_costs(GasCosts::free())
        .add_unsigned_coin_input(
            wallets[0],
            source_coin.utxo_id,
            source_coin.amount,
            source_coin.asset_id,
            TxPointer::new(source_coin.block_created.into(), source_coin.tx_created_idx),
        )
        .add_output(Output::Coin {
            to: target_owner.into(),
            amount: source_coin.amount,
            asset_id: source_coin.asset_id,
        })
        .finalize_as_transaction();
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
    let third_tx = TransactionBuilder::script(vec![], vec![])
        .max_fee_limit(0)
        .script_gas_limit(1_000_001) // changed to make it a different transaction
        .with_gas_costs(GasCosts::free())
        .add_unsigned_coin_input(
            wallets[0],
            source_coin.utxo_id,
            source_coin.amount,
            source_coin.asset_id,
            TxPointer::new(source_coin.block_created.into(), source_coin.tx_created_idx),
        )
        .add_output(Output::Coin {
            to: target_owner.into(),
            amount: source_coin.amount,
            asset_id: source_coin.asset_id,
        })
        .finalize_as_transaction();

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
