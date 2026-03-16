#![allow(non_snake_case)]

use fuel_core_client::client::types::TransactionStatus;
use fuel_core_types::fuel_types::BlockHeight;
use rand::{
    SeedableRng,
    prelude::StdRng,
};
use test_helpers::{
    fuel_core_driver::FuelCoreDriver,
    produce_block_with_tx,
};

#[tokio::test(flavor = "multi_thread")]
async fn history_retention__basic_pruning_on_restart() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(2322);

    // Start node without history retention
    let driver = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
    ])
    .await?;

    // Produce 10 blocks with transactions
    for _ in 0..10 {
        produce_block_with_tx(&mut rng, &driver.client).await;
    }

    // Verify block 1 is accessible
    let block_1 = driver.client.block_by_height(1u32.into()).await?;
    assert!(block_1.is_some(), "Block 1 should exist before pruning");

    // Stop the node, keep the db directory
    let db_dir = driver.kill().await;

    // Restart with very short history retention (1 second)
    let driver2 = FuelCoreDriver::spawn_feeless_with_directory(
        db_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--history-retention",
            "1s",
        ],
    )
    .await?;

    // Wait a moment for startup pruning to complete
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Old blocks should be pruned (block 1 should be gone)
    let block_1 = driver2.client.block_by_height(1u32.into()).await?;
    assert!(
        block_1.is_none(),
        "Block 1 should be pruned after restart with history-retention"
    );

    // Latest block should still be accessible
    let block_10 = driver2.client.block_by_height(10u32.into()).await?;
    assert!(
        block_10.is_some(),
        "Latest block should still be accessible"
    );

    driver2.kill().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn history_retention__ongoing_pruning_after_block_import() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(3456);

    // Start with short history retention
    let driver = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
        "--history-retention",
        "2s",
    ])
    .await?;

    // Produce initial blocks
    for _ in 0..5 {
        produce_block_with_tx(&mut rng, &driver.client).await;
    }

    // Wait for retention window to pass
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Produce more blocks to trigger pruning
    for _ in 0..5 {
        produce_block_with_tx(&mut rng, &driver.client).await;
    }

    // Give pruning a moment to run
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Early blocks should be pruned
    let block_1 = driver.client.block_by_height(1u32.into()).await?;
    assert!(
        block_1.is_none(),
        "Block 1 should have been pruned by ongoing pruning"
    );

    // Recent blocks should still exist
    let block_10 = driver.client.block_by_height(10u32.into()).await?;
    assert!(
        block_10.is_some(),
        "Recent block should still be accessible"
    );

    driver.kill().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn history_retention__node_works_after_pruning() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(7890);

    let driver = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
        "--history-retention",
        "1s",
    ])
    .await?;

    // Produce some blocks
    for _ in 0..5 {
        produce_block_with_tx(&mut rng, &driver.client).await;
    }

    // Wait for retention window
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Produce more blocks — this triggers pruning AND proves the node still works
    for _ in 0..5 {
        produce_block_with_tx(&mut rng, &driver.client).await;
    }

    // Verify the node can still produce new blocks after pruning
    produce_block_with_tx(&mut rng, &driver.client).await;

    // Verify the latest block is accessible
    let latest = driver.client.block_by_height(11u32.into()).await?;
    assert!(
        latest.is_some(),
        "Node should still serve new blocks after pruning"
    );

    driver.kill().await;
    Ok(())
}
