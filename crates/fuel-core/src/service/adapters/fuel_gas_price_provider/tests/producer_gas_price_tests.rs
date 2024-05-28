use super::*;

#[test]
fn gas_price__can_get_a_historical_gas_price() {
    // given
    let block_height = 432;
    let latest_height = (432 + 2).into();
    let expected_da_gas_price = 123;
    let expected_exec_gas_price = 0;
    let expected_gas_prices =
        GasPrices::new(expected_da_gas_price, expected_exec_gas_price);
    let gas_price_provider = ProviderBuilder::new()
        .with_latest_height(latest_height)
        .with_historical_gas_price(block_height.into(), expected_gas_prices)
        .build();

    // when
    let params = GasPriceParams::new(block_height.into());
    let actual = gas_price_provider.gas_price(params).unwrap();

    // then
    assert_eq!(actual, expected_gas_prices.total());
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
    assert!(maybe_price.is_err());
}

#[test]
fn gas_price__next_block_calls_algorithm_function() {
    // given
    let latest_height = 432;
    let latest_da_gas_price = 123;
    let latest_exec_gas_price = 0;
    let latest_gas_prices = GasPrices::new(latest_exec_gas_price, latest_da_gas_price);
    let next_height = (latest_height + 1).into();
    let cost = 100;
    let reward = cost - 1;
    let block_fullness = BlockFullness::new(1, 1);
    let gas_price_provider = ProviderBuilder::new()
        .with_historical_gas_price(latest_height.into(), latest_gas_prices)
        .with_latest_height(latest_height.into())
        .with_historical_block_fullness(latest_height.into(), block_fullness)
        .with_historical_da_recording_cost(next_height, cost)
        .with_total_as_of_block(latest_height.into(), reward, cost)
        .build();

    // when
    let params = GasPriceParams::new(next_height);
    let maybe_price = gas_price_provider.gas_price(params);

    // then
    let new_da_gas_price = SimpleGasPriceAlgorithm::default().calculate_da_gas_price(
        latest_da_gas_price,
        reward,
        cost,
    );
    let new_exec_gas_price = SimpleGasPriceAlgorithm::default()
        .calculate_execution_gas_price(latest_exec_gas_price, block_fullness);
    let expected = new_da_gas_price + new_exec_gas_price;
    let actual = maybe_price.unwrap();
    assert_eq!(actual, expected);
}

// TODO: Change to prop test, and generalize to simplify readability (use a loop or something)
#[test]
fn gas_price__if_total_is_for_old_block_then_update_to_latest_block() {
    // given
    let latest_height = 432;
    let total_block_height = latest_height - 2;
    let latest_da_gas_price = 123;
    let latest_exec_gas_price = 0;
    let latest_gas_prices = GasPrices::new(latest_exec_gas_price, latest_da_gas_price);
    let next_height = (latest_height + 1).into();
    let cost = 100;
    let reward = cost - 1;
    let block_fullness = BlockFullness::new(1, 1);
    let gas_price_provider = ProviderBuilder::new()
        .with_historical_gas_price((latest_height - 1).into(), latest_gas_prices)
        .with_historical_production_reward((latest_height - 1).into(), reward)
        .with_historical_da_recording_cost((latest_height - 1).into(), cost)
        .with_historical_gas_price(latest_height.into(), latest_gas_prices)
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
    let new_da_gas_price = SimpleGasPriceAlgorithm::default().calculate_da_gas_price(
        latest_da_gas_price,
        reward,
        cost,
    );
    let new_exec_gas_price = SimpleGasPriceAlgorithm::default()
        .calculate_execution_gas_price(latest_exec_gas_price, block_fullness);
    let expected = new_da_gas_price + new_exec_gas_price;
    let actual = maybe_price.unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn gas_price__if_da_behind_fuel_block_then_do_not_update_da_gas_price() {
    // given
    let latest_fuel_height = 432;
    let arb_previous_height_diff = 12;
    let latest_da_height = latest_fuel_height - arb_previous_height_diff;
    let latest_da_gas_price = 123;
    let latest_exec_gas_price = 321;
    let latest_gas_prices = GasPrices::new(latest_exec_gas_price, latest_da_gas_price);
    let next_height = (latest_fuel_height + 1).into();
    let cost = 100;
    let reward = cost - 1;
    let block_fullness = BlockFullness::new(3, 4);
    let gas_price_provider = ProviderBuilder::new()
        .with_historical_gas_price(latest_fuel_height.into(), latest_gas_prices)
        .with_latest_height(latest_fuel_height.into())
        .with_historical_block_fullness(latest_fuel_height.into(), block_fullness)
        .with_historical_da_recording_cost(latest_da_height.into(), cost)
        .with_total_as_of_block(latest_da_height.into(), reward, cost)
        .build();

    // when
    let params = GasPriceParams::new(next_height);
    let actual = gas_price_provider.gas_price(params).unwrap();

    // then
    let algo = SimpleGasPriceAlgorithm::default();
    let new_da_gas_price = latest_da_gas_price;
    let new_exec_gas_price =
        algo.calculate_execution_gas_price(latest_exec_gas_price, block_fullness);
    let expected = new_da_gas_price + new_exec_gas_price;

    assert_eq!(actual, expected);
}
