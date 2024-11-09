//! E2E tests for AWS KMS

use test_helpers::fuel_core_driver::FuelCoreDriver;

#[tokio::test]
async fn can_produce_blocks_with_kms() {
    
    let args = vec![
        "--debug",
        "--poa-instant",
        "true",
        "--consensus-aws-kms",
        kms_arn,
    ];
    let driver = FuelCoreDriver::spawn(&args).await.unwrap();
    driver.client.produce_blocks(1, None).await.unwrap();
    let block = driver.client.block_by_height(1u32.into()).await.unwrap();
    let _ = driver.kill().await;
}
