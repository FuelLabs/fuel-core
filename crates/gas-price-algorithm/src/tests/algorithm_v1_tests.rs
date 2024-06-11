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
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(starting_da_gas_price)
        .with_da_gas_price_denominator(da_gas_price_denominator)
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
    let da_gas_price_denominator = 10;
    let block_bytes = 500;
    let arb_value = 100;
    let smaller_starting_reward =
        starting_cost + block_bytes * latest_gas_per_byte - arb_value;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_da_gas_price_denominator(da_gas_price_denominator)
        .with_total_rewards(smaller_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let profit = (smaller_starting_reward as i64
        - (starting_cost + block_bytes * latest_gas_per_byte) as i64);
    let expected = starting_exec_gas_price
        + last_da_gas_price
        + profit.abs() as u64 / da_gas_price_denominator;
    assert_eq!(expected, actual);
}

#[test]
fn calculate__positive_profit_decrease_gas_price() {
    // given
    let starting_exec_gas_price = 100;
    let last_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_gas_price_denominator = 10;
    let block_bytes = 500;
    let arb_value = 100;
    let larger_starting_reward =
        starting_cost + block_bytes * latest_gas_per_byte + arb_value;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_da_gas_price_denominator(da_gas_price_denominator)
        .with_total_rewards(larger_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .build();

    // (10 * 500) + 500 > 1500
    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let profit = (larger_starting_reward as i64
        - (starting_cost + block_bytes * latest_gas_per_byte) as i64);
    let expected = starting_exec_gas_price + last_da_gas_price
        - profit.abs() as u64 / da_gas_price_denominator;
    assert_eq!(expected, actual);
}
