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
fn gas_price__if_gas_price_too_high_return_error() {
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
fn gas_price__next_block_calls_algorithm_function() {
    // given
    let latest_height = 432;
    let latest_gas_price = 123;
    let next_height = (latest_height + 1).into();
    let cost = 100;
    let reward = cost - 1;
    let block_fullness = BlockFullness::new(1, 1);
    let gas_price_provider = ProviderBuilder::new()
        .with_historical_gas_price(latest_height.into(), latest_gas_price)
        .with_latest_height(latest_height.into())
        .with_historical_block_fullness(latest_height.into(), block_fullness)
        .with_total_as_of_block(latest_height.into(), reward, cost)
        .build();

    // when
    let params = GasPriceParams::new(next_height);
    let maybe_price = gas_price_provider.gas_price(params);

    // then
    let expected = SimpleGasPriceAlgorithm::default().calculate_gas_price(
        latest_gas_price,
        reward,
        cost,
        block_fullness,
    );
    let actual = maybe_price.unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn gas_price__new_gas_price_never_exceeds_maximum_gas_price() {
    // given
    let latest_height = 432;
    let latest_gas_price = 123;
    let next_height = (latest_height + 1).into();
    let cost = 100;
    let reward = cost - 1;
    // 1000 > 1, so it will be capped at 1
    let flat_change = 1000;
    let max_change = 1;
    let block_fullness = BlockFullness::new(1, 1);
    let gas_price_provider = ProviderBuilder::new()
        .with_historical_gas_price(latest_height.into(), latest_gas_price)
        .with_latest_height(latest_height.into())
        .with_total_as_of_block(latest_height.into(), reward, cost)
        .with_historical_block_fullness(latest_height.into(), block_fullness)
        .with_alorithm_settings(1000, 1)
        .build();

    // when
    let params = GasPriceParams::new(next_height);
    let maybe_price = gas_price_provider.gas_price(params);

    // then
    let expected = SimpleGasPriceAlgorithm::new(flat_change, max_change)
        .maximum_next_gas_price(latest_gas_price);
    let actual = maybe_price.unwrap();
    assert_eq!(actual, expected);
}

// TODO: Change to prop test, and generalize to simplify readability (use a loop or something)
#[test]
fn gas_price__if_total_is_for_old_block_update_to_latest_block() {
    // given
    let latest_height = 432;
    let total_block_height = latest_height - 2;
    let latest_gas_price = 123;
    let next_height = (latest_height + 1).into();
    let cost = 100;
    let reward = cost - 1;
    let block_fullness = BlockFullness::new(1, 1);
    let gas_price_provider = ProviderBuilder::new()
        .with_historical_gas_price((latest_height - 1).into(), latest_gas_price)
        .with_historical_production_reward((latest_height - 1).into(), reward)
        .with_historical_da_recording_cost((latest_height - 1).into(), cost)
        .with_historical_gas_price(latest_height.into(), latest_gas_price)
        .with_historical_production_reward(latest_height.into(), reward)
        .with_historical_da_recording_cost(latest_height.into(), cost)
        .with_latest_height(latest_height.into())
        .with_historical_block_fullness(latest_height.into(), block_fullness)
        .with_total_as_of_block(total_block_height.into(), reward, cost)
        .build();

    // when
    let params = GasPriceParams::new(next_height);
    let maybe_price = gas_price_provider.gas_price(params);

    // then
    let expected = SimpleGasPriceAlgorithm::default().calculate_gas_price(
        latest_gas_price,
        reward,
        cost,
        block_fullness,
    );
    let actual = maybe_price.unwrap();
    assert_eq!(actual, expected);
}
