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
