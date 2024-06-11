use crate::service::adapters::fuel_gas_price_provider::{
    tests::{
        build_provider,
        TestGasPriceAlgorithm,
    },
};
use fuel_core_producer::block_producer::gas_price::GasPriceProvider as ProducerGasPriceProvider;
use fuel_core_gas_price_service::GasPriceAlgorithm;

#[tokio::test]
async fn gas_price__if_requested_block_height_is_latest_return_gas_price() {
    // given
    let algo = TestGasPriceAlgorithm::default();
    let gas_price_provider = build_provider(algo);
    let bytes = 10;

    // when
    let expected_price = algo.next_gas_price(bytes);
    let actual_price = gas_price_provider.gas_price(bytes).await.unwrap();

    // then
    assert_eq!(expected_price, actual_price);
}
