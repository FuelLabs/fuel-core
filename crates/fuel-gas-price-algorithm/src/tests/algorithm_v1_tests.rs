#![allow(clippy::cast_possible_truncation)]
use super::*;

#[test]
fn calculate__even_profit_maintains_price() {
    // given
    let starting_exec_gas_price = 100;
    let starting_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_gas_price_denominator = 1;
    let block_bytes = 500;
    let starting_reward = starting_cost + block_bytes * latest_gas_per_byte;
    let updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(starting_da_gas_price)
        .with_da_p_component(da_gas_price_denominator)
        .with_total_rewards(starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
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
    let block_bytes = 500;
    let profit_avg = 100;
    let avg_window = 10;
    let arb_value = 1000;
    let smaller_starting_reward =
        starting_cost + block_bytes * latest_gas_per_byte - arb_value;
    let updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_total_rewards(smaller_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .with_profit_avg(profit_avg, avg_window)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let profit = smaller_starting_reward as i64
        - (starting_cost + block_bytes * latest_gas_per_byte) as i64;
    let new_profit_avg =
        (profit_avg * (avg_window as i64 - 1) + profit) / avg_window as i64;

    let da_p_comp = new_profit_avg / da_p_component;
    let slope = new_profit_avg - profit_avg;
    let da_d_comp = slope / da_d_component;
    let expected =
        starting_exec_gas_price as i64 + last_da_gas_price as i64 - da_p_comp - da_d_comp;
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
    let block_bytes = 500;
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
        .with_total_rewards(larger_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .with_profit_avg(profit_avg, avg_window)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let profit = larger_starting_reward as i64
        - (starting_cost + block_bytes * latest_gas_per_byte) as i64;
    let new_profit_avg =
        (profit_avg * (avg_window as i64 - 1) + profit) / avg_window as i64;

    let da_p_comp = new_profit_avg / da_p_component;
    let slope = new_profit_avg - profit_avg;
    let da_d_comp = slope / da_d_component;
    let expected =
        starting_exec_gas_price as i64 + last_da_gas_price as i64 - da_p_comp - da_d_comp;
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
    let block_bytes = 500;
    let profit_avg = 100;
    let avg_window = 10;
    let max_change_percent = 5;
    let arb_value = 1000;
    let larger_starting_reward =
        starting_cost + block_bytes * latest_gas_per_byte + arb_value;
    let updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_total_rewards(larger_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .with_profit_avg(profit_avg, avg_window)
        .with_max_change_percent(max_change_percent)
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
fn calculate__price_does_not_increase_more_than_max_percent() {
    // given
    let starting_exec_gas_price = 100;
    let last_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_p_component = 100;
    let da_d_component = 10;
    let block_bytes = 500;
    let profit_avg = 100;
    let avg_window = 10;
    let arb_value = 1000;
    let max_change_percent = 5;
    let smaller_starting_reward =
        starting_cost + block_bytes * latest_gas_per_byte - arb_value;
    let updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_total_rewards(smaller_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .with_profit_avg(profit_avg, avg_window)
        .with_max_change_percent(max_change_percent)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let max_change =
        (starting_exec_gas_price as f64 * max_change_percent as f64 / 100.0) as i64;
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
    let block_bytes = 500;
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
        .with_total_rewards(larger_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .with_profit_avg(profit_avg, avg_window)
        .with_min_da_gas_price(min_da_gas_price)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let expected = min_da_gas_price + starting_exec_gas_price;
    assert_eq!(expected, actual);
}
