use super::*;

#[test]
fn gas_price__can_get_a_historical_gas_price() {
    // given
    let block_height = 432;
    let latest_height = (432 + 2).into();
    let expected = 123;
    let gas_price_provider = ProviderBuilder::new()
        .with_latest_height(latest_height)
        .with_historical_gas_price(block_height.into(), expected)
        .build();

    // when
    let params = GasPriceParams::new(block_height.into());
    let actual = gas_price_provider.gas_price(params).unwrap();

    // then
    assert_eq!(actual, expected);
}

#[test]
fn gas_price__if_gas_price_too_new_return_none() {
    // given
    let latest_height = 432;
    let too_new_height = (latest_height + 2).into();
    let gas_price_provider = ProviderBuilder::new()
        .with_latest_height(latest_height.into())
        .build();

    // when
    let params = GasPriceParams::new(too_new_height);
    let maybe_price = gas_price_provider.gas_price(params);

    // then
    assert!(maybe_price.is_none());
}

#[test]
fn gas_price__if_next_gas_price_return_some() {
    // given
    let latest_height = 432;
    let next_height = (latest_height + 1).into();
    let gas_price_provider = ProviderBuilder::new()
        .with_latest_height(latest_height.into())
        .build();

    // when
    let params = GasPriceParams::new(next_height);
    let maybe_price = gas_price_provider.gas_price(params);

    // then
    assert!(maybe_price.is_some());
}
