use crate::v1::tests::UpdaterBuilder;

#[test]
fn calculate__even_profit_maintains_price() {
    // given
    let starting_exec_gas_price = 100;
    let starting_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_gas_price_denominator = 1;
    let block_bytes = 500u64;
    let starting_reward = starting_cost + block_bytes * latest_gas_per_byte;
    let updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(starting_da_gas_price)
        .with_da_p_component(da_gas_price_denominator)
        .with_total_rewards(starting_reward as u128)
        .with_known_total_cost(starting_cost as u128)
        .with_projected_total_cost(starting_cost as u128)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let expected = starting_exec_gas_price + starting_da_gas_price;
    assert_eq!(expected, actual);
}

#[test]
fn calculate__negative_profit_increase_gas_price() {
    // given
    let starting_exec_gas_price = 100;
    let last_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_p_component = 100;
    let da_d_component = 10;
    let block_bytes = 500u64;
    let last_profit = -100;
    let last_last_profit = 0;
    let arb_value = 1000;
    let smaller_starting_reward =
        starting_cost + block_bytes * latest_gas_per_byte - arb_value;
    let updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_total_rewards(smaller_starting_reward as u128)
        .with_known_total_cost(starting_cost as u128)
        .with_projected_total_cost(starting_cost as u128)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .with_last_profit(last_profit, last_last_profit)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let da_p_comp = last_profit / da_p_component as i128;
    let slope = last_profit - last_last_profit;
    let da_d_comp = slope / da_d_component as i128;
    let expected = starting_exec_gas_price as i128 + last_da_gas_price as i128
        - da_p_comp
        - da_d_comp;
    assert_eq!(expected as u64, actual);
}

#[test]
fn calculate__positive_profit_decrease_gas_price() {
    // given
    let starting_exec_gas_price = 100;
    let last_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_p_component = 100;
    let da_d_component = 10;
    let block_bytes = 500u64;
    let last_profit = 100;
    let last_last_profit = 0;
    let arb_value = 1000;
    let larger_starting_reward =
        starting_cost + block_bytes * latest_gas_per_byte + arb_value;
    let updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_total_rewards(larger_starting_reward as u128)
        .with_known_total_cost(starting_cost as u128)
        .with_projected_total_cost(starting_cost as u128)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .with_last_profit(last_profit, last_last_profit)
        .with_da_max_change_percent(u16::MAX)
        .with_exec_gas_price_change_percent(0)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let da_p_comp = last_profit / da_p_component as i128;
    let slope = last_profit - last_last_profit;
    let da_d_comp = slope / da_d_component as i128;
    let expected = starting_exec_gas_price as i128 + last_da_gas_price as i128
        - da_p_comp
        - da_d_comp;
    assert_eq!(expected as u64, actual);
}

#[test]
fn calculate__price_does_not_decrease_more_than_max_percent() {
    // given
    let starting_exec_gas_price = 100;
    let last_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_p_component = 100;
    let da_d_component = 10;
    let block_bytes = 500u64;
    let last_profit = 100000; // Large, positive profit to decrease da price
    let last_last_profit = 0;
    let max_change_percent = 5;
    let arb_value = 1000;
    let larger_starting_reward =
        starting_cost + block_bytes * latest_gas_per_byte + arb_value;
    let updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_total_rewards(larger_starting_reward as u128)
        .with_known_total_cost(starting_cost as u128)
        .with_projected_total_cost(starting_cost as u128)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .with_last_profit(last_profit, last_last_profit)
        .with_da_max_change_percent(max_change_percent)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let max_change =
        (starting_exec_gas_price as f64 * max_change_percent as f64 / 100.0) as i64;
    let expected = starting_exec_gas_price as i64 + last_da_gas_price as i64 - max_change;
    assert_eq!(expected as u64, actual);
}
#[test]
fn calculate__da_price_does_not_increase_more_than_max_percent() {
    // given
    let starting_exec_gas_price = 100;
    let last_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_p_component = 1000;
    let da_d_component = 10;
    let block_bytes = 500u64;
    let last_profit = -1000000; // large, negative profit to increase da price
    let last_last_profit = 0;
    let arb_value = 1000;
    let max_da_change_percent = 5;
    let max_exec_change_percent = 0;
    let smaller_starting_reward =
        starting_cost + block_bytes * latest_gas_per_byte - arb_value;
    let updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_total_rewards(smaller_starting_reward as u128)
        .with_known_total_cost(starting_cost as u128)
        .with_projected_total_cost(starting_cost as u128)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .with_last_profit(last_profit, last_last_profit)
        .with_da_max_change_percent(max_da_change_percent)
        .with_exec_gas_price_change_percent(max_exec_change_percent)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let max_change =
        (last_da_gas_price as f64 * max_da_change_percent as f64 / 100.0) as i64;
    let expected = starting_exec_gas_price as i64 + last_da_gas_price as i64 + max_change;

    assert_eq!(expected as u64, actual);
}

#[test]
fn calculate__da_gas_price_never_drops_below_minimum() {
    // given
    let starting_exec_gas_price = 100;
    let last_da_gas_price = 100;
    let min_da_gas_price = 99;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_p_component = 100;
    let da_d_component = 10;
    let block_bytes = 500u64;
    let profit_avg = 100;
    let avg_window = 10;
    let arb_value = 1000;
    let larger_starting_reward =
        starting_cost + block_bytes * latest_gas_per_byte + arb_value;
    let updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_total_rewards(larger_starting_reward as u128)
        .with_known_total_cost(starting_cost as u128)
        .with_projected_total_cost(starting_cost as u128)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .with_last_profit(profit_avg, avg_window)
        .with_min_da_gas_price(min_da_gas_price)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let expected = min_da_gas_price + starting_exec_gas_price;
    assert_eq!(expected, actual);
}
