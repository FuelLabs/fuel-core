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
        .build_with_simple_algo();

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
        .build_with_simple_algo();

    // when
    let params = GasPriceParams::new(too_new_height);
    let maybe_price = gas_price_provider.gas_price(params);

    // then
    assert!(maybe_price.is_none());
}

#[test]
fn gas_price__if_asking_for_next_gas_price_return_some() {
    // given
    let latest_height = 432;
    let arbitrary_gas_price = 555;
    let next_height = (latest_height + 1).into();
    let gas_price_provider = ProviderBuilder::new()
        .with_historical_gas_price(latest_height.into(), arbitrary_gas_price)
        .with_latest_height(latest_height.into())
        .with_total_as_of_block(latest_height.into(), 0, 0)
        .build_with_simple_algo();

    // when
    let params = GasPriceParams::new(next_height);
    let maybe_price = gas_price_provider.gas_price(params);

    // then
    assert!(maybe_price.is_some());
}

#[test]
fn gas_price__if_profit_is_negative_increase_price() {
    // given
    let latest_height = 432;
    let latest_gas_price = 123;
    let next_height = (latest_height + 1).into();
    let cost = 100;
    let reward = cost - 1;
    let gas_price_provider = ProviderBuilder::new()
        .with_historical_gas_price(latest_height.into(), latest_gas_price)
        .with_latest_height(latest_height.into())
        .with_total_as_of_block(latest_height.into(), reward, cost)
        .build_with_simple_algo();

    // when
    let params = GasPriceParams::new(next_height);
    let maybe_price = gas_price_provider.gas_price(params);

    // then
    let expected = SimpleGasPriceAlgorithm.calculate_gas_price(
        latest_gas_price,
        reward,
        cost,
        BlockFullness,
    );
    let actual = maybe_price.unwrap();
    assert_eq!(actual, expected);
}
