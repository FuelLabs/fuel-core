#![allow(non_snake_case)]

use fuel_core_storage::transactional::HistoricalView;
use fuel_core_types::fuel_types::BlockHeight;
use proptest::{
    prelude::{
        Just,
        ProptestConfig,
    },
    prop_compose,
    proptest,
};
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
        database.on_chain().latest_height(),
        Some(BlockHeight::new(HEIGHTS))
    );
    for _ in 0..HEIGHTS {
        database.off_chain().rollback_last_block()?;
    }
    assert!(database.on_chain().latest_height() > database.off_chain().latest_height());
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
        recovered_database.on_chain().latest_height(),
        Some(BlockHeight::new(HEIGHTS))
    );
    assert_eq!(
        recovered_database.off_chain().latest_height(),
        Some(BlockHeight::new(HEIGHTS))
    );

    Ok(())
}

prop_compose! {
    fn height_and_lower_height()(height in 2..100u32)(height in Just(height), lower_height in 1..height) -> (u32, u32) {
        (height, lower_height)
    }
}

async fn _gas_price_updater__can_recover_on_startup_when_gas_price_db_is_ahead(
    height: u32,
    lower_height: u32,
) -> anyhow::Result<()> {
    let driver = FuelCoreDriver::spawn(&[
        "--debug",
        "--poa-instant",
        "true",
        "--state-rewind-duration",
        "7d",
    ])
    .await?;

    // Given
    driver.client.produce_blocks(height, None).await?;
    let database = &driver.node.shared.database;
    assert_eq!(
        database.on_chain().latest_height(),
        Some(BlockHeight::new(height))
    );
    let diff = height - lower_height;
    for _ in 0..diff {
        database.on_chain().rollback_last_block()?;
        database.off_chain().rollback_last_block()?;
    }
    assert!(database.on_chain().latest_height() < database.gas_price().latest_height());
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
    let actual_onchain_height = recovered_database.on_chain().latest_height().unwrap();
    let expected_onchain_height = BlockHeight::new(lower_height);

    let actual_gas_price_height = recovered_database
        .gas_price()
        .latest_height()
        .unwrap_or(0.into()); // Gas price metadata never gets written for block 0
    let expected_gas_price_height = BlockHeight::new(lower_height);

    assert_eq!(actual_onchain_height, expected_onchain_height);
    assert_eq!(actual_gas_price_height, expected_gas_price_height);

    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]
    #[test]
    fn gas_price_updater__can_recover_on_startup_when_gas_price_db_is_ahead((height, lower_height) in height_and_lower_height()) {
        let rt = multithreaded_runtime();
        rt.block_on(
            _gas_price_updater__can_recover_on_startup_when_gas_price_db_is_ahead(height, lower_height)
        ).unwrap()
    }
}
async fn _gas_price_updater__can_recover_on_startup_when_gas_price_db_is_behind(
    height: u32,
    lower_height: u32,
) -> anyhow::Result<()> {
    let driver = FuelCoreDriver::spawn(&[
        "--debug",
        "--poa-instant",
        "true",
        "--state-rewind-duration",
        "7d",
    ])
    .await?;

    // Given
    driver.client.produce_blocks(height, None).await?;
    let database = &driver.node.shared.database;
    assert_eq!(
        database.on_chain().latest_height(),
        Some(BlockHeight::new(height))
    );

    let diff = height - lower_height;
    for _ in 0..diff {
        let _ = database.gas_price().rollback_last_block();
    }
    assert!(database.on_chain().latest_height() > database.gas_price().latest_height());
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
        recovered_database.on_chain().latest_height(),
        Some(BlockHeight::new(height))
    );
    assert_eq!(
        recovered_database.gas_price().latest_height(),
        Some(BlockHeight::new(height))
    );

    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]
    #[test]
    fn gas_price_updater__can_recover_on_startup_when_gas_price_db_is_behind((height, lower_height) in height_and_lower_height()) {
        let rt = multithreaded_runtime();
        rt.block_on(
            _gas_price_updater__can_recover_on_startup_when_gas_price_db_is_behind(height, lower_height)
        ).unwrap()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn gas_price_updater__if_no_metadata_history_start_from_current_block(
) -> anyhow::Result<()> {
    let driver = FuelCoreDriver::spawn(&[
        "--debug",
        "--poa-instant",
        "true",
        "--state-rewind-duration",
        "7d",
    ])
    .await?;

    // Given
    let height = 100;
    let lower_height = 0;
    driver.client.produce_blocks(height, None).await?;
    let database = &driver.node.shared.database;
    assert_eq!(
        database.on_chain().latest_height(),
        Some(BlockHeight::new(height))
    );

    let diff = height - lower_height;
    for _ in 0..diff {
        let _ = database.gas_price().rollback_last_block();
    }
    assert!(database.on_chain().latest_height() > database.gas_price().latest_height());
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
    // advance the block height to the next block to add the metadata to db
    recovered_driver.client.produce_blocks(1, None).await?;
    // And wait for the gas price updater to update the gas price metadata.
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;

    let recovered_database = &recovered_driver.node.shared.database;
    let next_height = height + 1;
    assert_eq!(
        recovered_database.on_chain().latest_height(),
        Some(BlockHeight::new(next_height))
    );
    assert_eq!(
        recovered_database.gas_price().latest_height(),
        Some(BlockHeight::new(next_height))
    );

    Ok(())
}

fn multithreaded_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}
