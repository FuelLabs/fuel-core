use super::*;
use fuel_core_gas_price_service::static_updater::StaticAlgorithm;

#[tokio::test]
async fn estimate_gas_price__happy_path() {
    // given
    let next_height = 432.into();
    let price = 33;
    let algo = StaticAlgorithm::new(price);
    let gas_price_provider = build_provider(algo.clone());

    // when
    let expected_price = algo.worst_case_gas_price(next_height);
    let actual_price = gas_price_provider
        .worst_case_gas_price(next_height)
        .await
        .unwrap();

    // then
    assert_eq!(expected_price, actual_price);
}
