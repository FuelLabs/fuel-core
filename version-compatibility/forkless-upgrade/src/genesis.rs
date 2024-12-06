#![allow(non_snake_case)]
use crate::tests_helper::{
    LatestFuelCoreDriver,
    IGNITION_TESTNET_SNAPSHOT,
};
use latest_fuel_core_type::fuel_tx::Bytes32;
use std::str::FromStr;

#[tokio::test(flavor = "multi_thread")]
async fn test__genesis_block__hash() {
    let latest_node = LatestFuelCoreDriver::spawn(&[
        "--debug",
        "--poa-instant",
        "true",
        "--snapshot",
        IGNITION_TESTNET_SNAPSHOT,
        "--enable-relayer",
        "--relayer",
        "https://eth-mainnet.public.blastapi.io",
        "--relayer-da-deploy-height",
        "5791365",
        "--relayer-v2-listening-contracts",
        "0x768f9459E3339A1F7d59CcF24C80Eb4A711a01FB",
    ])
    .await
    .unwrap();

    // Given
    let original_block = latest_node
        .client
        .block_by_height(0u32.into())
        .await
        .expect("Failed to get blocks")
        .expect("Genesis block should exists");
    assert_eq!(
        original_block.id,
        Bytes32::from_str(
            "0x19ac99bf59711aca047b28443e599e26f733291c2fa45f5f309b2c5c9712b215"
        )
        .unwrap()
    )
}
