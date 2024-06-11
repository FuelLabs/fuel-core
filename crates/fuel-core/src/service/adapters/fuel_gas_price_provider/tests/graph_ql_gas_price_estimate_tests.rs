use super::*;

#[tokio::test]
async fn estimate_gas_price__happy_path() {
    // given
    let next_height = 432.into();
    let algo = TestGasPriceAlgorithm::default();
    let gas_price_provider = build_provider(algo);

    // when
    let expected_price = algo.worst_case_gas_price(next_height);
    let actual_price = gas_price_provider.worst_case_gas_price(next_height).await;

    // then
    assert_eq!(expected_price, actual_price);
}
