#![allow(non_snake_case)]

use clap::Parser;
use fuel_core::{
    schema::tx::types::TransactionStatus,
    service::{
        config::fuel_core_importer::ports::Validator,
        FuelService,
    },
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_txpool::types::TxId;
use fuel_core_types::{
    fuel_tx::{
        AssetId,
        Input,
        Output,
        Transaction,
        TransactionBuilder,
        UniqueIdentifier,
    },
    fuel_types::BlockHeight,
};
use futures::StreamExt;
use itertools::Itertools;
use rand::{
    prelude::StdRng,
    Rng,
    SeedableRng,
};
use std::collections::BTreeSet;
use test_helpers::{
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
    const TOTAL_BLOCKS: u64 = 5000;
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
        assert!(matches!(result, TransactionStatus::Success(_)));
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
