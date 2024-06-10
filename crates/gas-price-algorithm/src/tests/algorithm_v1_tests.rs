use super::*;

#[test]
fn calculate__if_above_capacity_threshold_increase_price() {
    // given
    let starting_gas_price = 100;
    let threshold = 50;
    let increase_amount = 10;
    let fullness = (60, 100);
    let mut updater = UpdaterBuilder::new()
        .with_starting_gas_price(starting_gas_price)
        .with_l2_block_capacity_threshold(threshold)
        .with_l2_comp_gas_price_increase_amount(increase_amount)
        .with_last_l2_fullness(fullness)
        .build();

    let ignore_block_bytes = 0;
    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(ignore_block_bytes);

    // then
    let expected = starting_gas_price + increase_amount;
    assert_eq!(actual, expected);
}

#[test]
fn calculate__if_below_capacity_threshold_decrease_price() {
    // given
    let starting_gas_price = 100;
    let threshold = 50;
    let increase_amount = 10;
    let fullness = (40, 100);
    let mut updater = UpdaterBuilder::new()
        .with_starting_gas_price(starting_gas_price)
        .with_l2_block_capacity_threshold(threshold)
        .with_l2_comp_gas_price_increase_amount(increase_amount)
        .with_last_l2_fullness(fullness)
        .build();

    let ignore_block_bytes = 0;
    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(ignore_block_bytes);

    // then
    let expected = starting_gas_price - increase_amount;
    assert_eq!(actual, expected);
}

#[test]
fn calculate__positive_profit_decreases_gas_price() {
    // given
    let starting_gas_price = 100;
    let starting_reward = 1500;
    let starting_cost = 500;
    let latest_gas_per_byte = 1;
    let da_gas_price_denominator = 10;
    let mut updater = UpdaterBuilder::new()
        .with_starting_gas_price(starting_gas_price)
        .with_da_gas_price_denominator(da_gas_price_denominator)
        .with_total_rewards(starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .build();

    let block_bytes = 500;

    // (1 * 500) + 500 < 1500
    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let profit = (starting_reward as i64 - (starting_cost + block_bytes * latest_gas_per_byte) as i64) as u64;
    let expected = starting_gas_price - profit / da_gas_price_denominator;
    assert_eq!(expected, actual);
}

#[test]
fn calculate__negative_profit_increase_gas_price() {

    // given
    let starting_gas_price = 100;
    let starting_reward = 1500;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_gas_price_denominator = 10;
    let mut updater = UpdaterBuilder::new()
        .with_starting_gas_price(starting_gas_price)
        .with_da_gas_price_denominator(da_gas_price_denominator)
        .with_total_rewards(starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .build();

    let block_bytes = 500;

    // (10 * 500) + 500 > 1500
    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let profit = (starting_reward as i64 - (starting_cost + block_bytes * latest_gas_per_byte) as i64);
    let expected = starting_gas_price + profit.abs() as u64 / da_gas_price_denominator;
    assert_eq!(expected, actual);
}
