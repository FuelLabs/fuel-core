use crate::service::adapters::fuel_gas_price_provider::tests::{
    build_provider,
    TestGasPriceAlgorithm,
};
use fuel_core_gas_price_service::GasPriceAlgorithm;

#[tokio::test]
async fn gas_price__if_requested_block_height_is_latest_return_gas_price() {
    // given
    let algo = TestGasPriceAlgorithm::default();
    let gas_price_provider = build_provider(algo);

    // when
    let expected_price = algo.last_gas_price();
    let actual_price = gas_price_provider.last_gas_price().await;

    // then
    assert_eq!(expected_price, actual_price);
}
