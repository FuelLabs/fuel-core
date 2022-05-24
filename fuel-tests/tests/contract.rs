use crate::helpers::{TestContext, TestSetupBuilder};
use fuel_vm::prelude::*;

const SEED: u64 = 2322;

#[tokio::test]
async fn test_contract_salt() {
    let mut test_builder = TestSetupBuilder::new(SEED);
    let (_, contract_id) = test_builder.setup_contract(vec![], None);

    // spin up node
    let TestContext { client, .. } = test_builder.finalize().await;

    let contract = client
        .contract(format!("{:#x}", contract_id).as_str())
        .await
        .unwrap();

    // Check that salt is 0x Hex prefixed
    let salt = contract.unwrap().salt;
    assert_eq!("0x", &salt.to_string()[..2]);
}

#[tokio::test]
async fn test_contract_balance() {
    for test_bal in 0..10 {
        let mut test_builder = TestSetupBuilder::new(SEED);
        let (_, contract_id) =
            test_builder.setup_contract(vec![], Some(vec![(AssetId::new([1u8; 32]), test_bal)]));

        // spin up node
        let TestContext { client, .. } = test_builder.finalize().await;

        let asset_id = AssetId::new([1u8; 32]);

        let balance = client
            .contract_balance(
                format!("{:#x}", contract_id).as_str(),
                Some(format!("{:#x}", asset_id).as_str()),
            )
            .await
            .unwrap();

        assert_eq!(balance, test_bal);
    }
}
