use super::*;

#[test]
fn gas_price__if_requested_block_height_too_high_return_error() {
    // given
    let latest_height = 432;
    let too_new_height = (latest_height + 2).into();
    let gas_price_provider =
        build_provider(latest_height.into(), SimpleGasPriceAlgorithm::default());
    let bytes = 0;

    // when
    let params = GasPriceParams::new(too_new_height, bytes);
    let maybe_price = gas_price_provider.gas_price(params);

    // then
    assert!(maybe_price.is_err());
}

#[test]
fn gas_price__if_requested_block_height_too_old_return_error() {
    // given
    let latest_height = 432;
    let too_old_height = (latest_height - 2).into();
    let gas_price_provider =
        build_provider(latest_height.into(), SimpleGasPriceAlgorithm::default());
    let bytes = 0;

    // when
    let params = GasPriceParams::new(too_old_height, bytes);
    let maybe_price = gas_price_provider.gas_price(params);

    // then
    assert!(maybe_price.is_err());
}

#[test]
fn gas_price__if_requested_block_height_is_latest_return_gas_price() {
    // given
    let latest_height = 432;
    let algo = SimpleGasPriceAlgorithm::default();
    let gas_price_provider = build_provider(latest_height.into(), algo);
    let bytes = 10;

    // when
    let params = GasPriceParams::new(latest_height.into(), bytes);
    let expected_price = algo.gas_price(bytes);
    let actual_price = gas_price_provider.gas_price(params).unwrap();

    // then
    assert_eq!(expected_price, actual_price);
}
