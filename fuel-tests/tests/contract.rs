use crate::helpers::{
    TestContext,
    TestSetupBuilder,
};
use fuel_core_interfaces::common::fuel_vm::prelude::*;
use fuel_gql_client::client::{
    PageDirection,
    PaginationRequest,
};
use rstest::rstest;

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

#[rstest]
#[tokio::test]
async fn test_contract_balance(
    #[values(AssetId::new([1u8; 32]), AssetId::new([0u8; 32]), AssetId::new([16u8; 32]))]
    asset: AssetId,
    #[values(100, 0, 18446744073709551615)] test_balance: u64,
) {
    let mut test_builder = TestSetupBuilder::new(SEED);
    let (_, contract_id) =
        test_builder.setup_contract(vec![], Some(vec![(asset, test_balance)]));

    // spin up node
    let TestContext { client, .. } = test_builder.finalize().await;

    let balance = client
        .contract_balance(
            format!("{:#x}", contract_id).as_str(),
            Some(format!("{:#x}", asset).as_str()),
        )
        .await
        .unwrap();

    assert_eq!(balance, test_balance);
}

#[rstest]
#[tokio::test]
async fn test_5_contract_balances(
    #[values(PageDirection::Forward, PageDirection::Backward)] direction: PageDirection,
) {
    let mut test_builder = TestSetupBuilder::new(SEED);
    let (_, contract_id) = test_builder.setup_contract(
        vec![],
        Some(vec![
            (AssetId::new([1u8; 32]), 1000),
            (AssetId::new([2u8; 32]), 400),
            (AssetId::new([3u8; 32]), 700),
        ]),
    );

    let TestContext { client, .. } = test_builder.finalize().await;

    let contract_balances = client
        .contract_balances(
            format!("{:#x}", contract_id).as_str(),
            PaginationRequest {
                cursor: None,
                results: 3,
                direction,
            },
        )
        .await
        .unwrap();

    assert!(!contract_balances.results.is_empty());
    if direction == PageDirection::Forward {
        assert_eq!(contract_balances.results[0].amount.0, 1000);
        assert_eq!(contract_balances.results[1].amount.0, 400);
        assert_eq!(contract_balances.results[2].amount.0, 700);
    } else {
        assert_eq!(contract_balances.results[2].amount.0, 1000);
        assert_eq!(contract_balances.results[1].amount.0, 400);
        assert_eq!(contract_balances.results[0].amount.0, 700);
    }
}
