#![allow(non_snake_case)]
use fuel_core_types::fuel_types::BlockHeight;
use test_helpers::fuel_core_driver::FuelCoreDriver;

#[tokio::test(flavor = "multi_thread")]
async fn off_chain_worker_can_recover_on_start_up_when_is_behind() -> anyhow::Result<()> {
    const HEIGHTS: u32 = 100;
    let driver = FuelCoreDriver::spawn(&[
        "--debug",
        "--poa-instant",
        "true",
        "--state-rewind-duration",
        "7d",
    ])
    .await?;

    // Given
    driver.client.produce_blocks(HEIGHTS, None).await?;
    let database = &driver.node.shared.database;
    assert_eq!(
        database.on_chain().latest_height()?,
        Some(BlockHeight::new(HEIGHTS))
    );
    for _ in 0..HEIGHTS {
        let _ = database.off_chain().rollback_last_block();
    }
    assert!(database.on_chain().latest_height()? > database.off_chain().latest_height()?);
    let temp_dir = driver.kill().await;

    // When
    let recovered_driver = FuelCoreDriver::spawn_with_directory(
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

    // Then
    let recovered_database = &recovered_driver.node.shared.database;
    assert_eq!(
        recovered_database.on_chain().latest_height()?,
        Some(BlockHeight::new(HEIGHTS))
    );
    assert_eq!(
        recovered_database.off_chain().latest_height()?,
        Some(BlockHeight::new(HEIGHTS))
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn gas_price_updater__can_recover_on_startup_when_gas_price_db_is_behind(
) -> anyhow::Result<()> {
    const HEIGHTS: u32 = 100;
    let driver = FuelCoreDriver::spawn(&[
        "--debug",
        "--poa-instant",
        "true",
        "--state-rewind-duration",
        "7d",
    ])
    .await?;

    // Given
    driver.client.produce_blocks(HEIGHTS, None).await?;
    let database = &driver.node.shared.database;
    assert_eq!(
        database.on_chain().latest_height()?,
        Some(BlockHeight::new(HEIGHTS))
    );
    for _ in 0..HEIGHTS {
        let _ = database.gas_price().rollback_last_block();
    }
    assert!(database.on_chain().latest_height()? > database.gas_price().latest_height()?);
    let temp_dir = driver.kill().await;

    // When
    let recovered_driver = FuelCoreDriver::spawn_with_directory(
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

    // Then
    let recovered_database = &recovered_driver.node.shared.database;
    assert_eq!(
        recovered_database.on_chain().latest_height()?,
        Some(BlockHeight::new(HEIGHTS))
    );
    assert_eq!(
        recovered_database.gas_price().latest_height()?,
        Some(BlockHeight::new(HEIGHTS))
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn gas_price_updater__can_recover_on_startup_when_gas_price_db_is_ahead(
) -> anyhow::Result<()> {
    const HEIGHTS: u32 = 100;
    const REVERT: u32 = HEIGHTS / 2;
    let driver = FuelCoreDriver::spawn(&[
        "--debug",
        "--poa-instant",
        "true",
        "--state-rewind-duration",
        "7d",
    ])
    .await?;

    // Given
    driver.client.produce_blocks(HEIGHTS, None).await?;
    let database = &driver.node.shared.database;
    assert_eq!(
        database.on_chain().latest_height()?,
        Some(BlockHeight::new(HEIGHTS))
    );
    for _ in 0..REVERT {
        let _ = database.on_chain().rollback_last_block();
        let _ = database.off_chain().rollback_last_block();
    }
    assert!(database.on_chain().latest_height()? < database.gas_price().latest_height()?);
    let temp_dir = driver.kill().await;

    // When
    let recovered_driver = FuelCoreDriver::spawn_with_directory(
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

    // Then
    let recovered_database = &recovered_driver.node.shared.database;
    assert_eq!(
        recovered_database.on_chain().latest_height()?,
        Some(BlockHeight::new(HEIGHTS - REVERT))
    );
    assert_eq!(
        recovered_database.gas_price().latest_height()?,
        Some(BlockHeight::new(HEIGHTS - REVERT))
    );

    Ok(())
}
