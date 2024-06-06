use crate::service::adapters::fuel_gas_price_provider::{
    ports::GasPriceAlgorithm,
    tests::{
        build_provider,
        TestGasPriceAlgorithm,
    },
};
use fuel_core_producer::block_producer::gas_price::GasPriceProvider as ProducerGasPriceProvider;

#[tokio::test]
async fn gas_price__if_requested_block_height_too_high_return_error() {
    // given
    let latest_height = 432;
    let too_new_height = (latest_height + 2).into();
    let gas_price_provider =
        build_provider(latest_height.into(), TestGasPriceAlgorithm::default());
    let bytes = 0;

    // when
    let maybe_price = gas_price_provider.gas_price(too_new_height, bytes).await;

    // then
    assert!(maybe_price.is_err());
}

#[tokio::test]
async fn gas_price__if_requested_block_height_too_old_return_error() {
    // given
    let latest_height = 432;
    let too_old_height = (latest_height - 2).into();
    let gas_price_provider =
        build_provider(latest_height.into(), TestGasPriceAlgorithm::default());
    let bytes = 0;

    // when
    let maybe_price = gas_price_provider.gas_price(too_old_height, bytes).await;

    // then
    assert!(maybe_price.is_err());
}

#[tokio::test]
async fn gas_price__if_requested_block_height_is_latest_return_gas_price() {
    // given
    let latest_height = 432.into();
    let algo = TestGasPriceAlgorithm::default();
    let gas_price_provider = build_provider(latest_height, algo);
    let bytes = 10;

    // when
    let expected_price = algo.gas_price(bytes);
    let actual_price = gas_price_provider
        .gas_price(latest_height, bytes)
        .await
        .unwrap();

    // then
    assert_eq!(expected_price, actual_price);
}
