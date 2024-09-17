use crate::v1::{
    tests::UpdaterBuilder,
    Error,
};

#[test]
fn update_l2_block_data__updates_l2_block() {
    // given
    let starting_block = 0;

    let mut updater = UpdaterBuilder::new()
        .with_l2_block_height(starting_block)
        .build();

    let height = 1;
    let used = 50;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let new_gas_price = 100;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, new_gas_price)
        .unwrap();

    //  then
    let expected = starting_block + 1;
    let actual = updater.l2_block_height;
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__skipped_block_height_throws_error() {
    // given
    let starting_block = 0;
    let mut updater = UpdaterBuilder::new()
        .with_l2_block_height(starting_block)
        .build();

    let height = 2;
    let used = 50;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let new_gas_price = 100;

    // when
    let actual_error = updater
        .update_l2_block_data(height, used, capacity, block_bytes, new_gas_price)
        .unwrap_err();

    // then
    let expected_error = Error::SkippedL2Block {
        expected: starting_block + 1,
        got: 2,
    };
    assert_eq!(actual_error, expected_error);
}

#[test]
fn update_l2_block_data__updates_projected_cost() {
    // given
    let da_cost_per_byte = 20;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .build();

    let height = 1;
    let used = 50;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let new_gas_price = 100;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, new_gas_price)
        .unwrap();

    // then
    let expected = block_bytes as u128 * da_cost_per_byte;
    let actual = updater.projected_total_da_cost;
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__updates_the_total_reward_value() {
    // given
    let starting_exec_gas_price = 100;
    let starting_da_gas_price = 10;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(starting_da_gas_price)
        .build();

    let height = 1;
    let gas_used = 50;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let new_gas_price = 200;

    // when
    updater
        .update_l2_block_data(height, gas_used, capacity, block_bytes, new_gas_price)
        .unwrap();

    // then
    let expected = gas_used * starting_da_gas_price;
    let actual = updater.total_da_rewards_excess;
    assert_eq!(actual, expected as u128);
}

#[test]
fn update_l2_block_data__even_threshold_will_not_change_exec_gas_price() {
    // given
    let starting_gas_price = 100;
    let unused_percent = 11;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_gas_price)
        .with_exec_gas_price_change_percent(unused_percent)
        .build();

    let height = 1;
    let used = 50;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let new_gas_price = 200;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, new_gas_price)
        .unwrap();

    // then
    let expected = starting_gas_price;
    let actual = updater.new_scaled_exec_price;
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__below_threshold_will_decrease_exec_gas_price() {
    // given
    let starting_exec_gas_price = 222;
    let exec_gas_price_decrease_percent = 10;
    let threshold = 50;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_exec_gas_price_change_percent(exec_gas_price_decrease_percent)
        .with_l2_block_capacity_threshold(threshold)
        .build();

    let height = 1;
    let used = 40;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let new_gas_price = 200;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, new_gas_price)
        .unwrap();

    // then
    let expected_change_amount =
        starting_exec_gas_price * exec_gas_price_decrease_percent as u64 / 100;
    let expected = starting_exec_gas_price - expected_change_amount;
    let actual = updater.new_scaled_exec_price;
    assert_eq!(expected, actual);
}

#[test]
fn update_l2_block_data__above_threshold_will_increase_exec_gas_price() {
    // given
    let starting_exec_gas_price = 222;
    let exec_gas_price_increase_percent = 10;
    let threshold = 50;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_exec_gas_price_change_percent(exec_gas_price_increase_percent)
        .with_l2_block_capacity_threshold(threshold)
        .build();

    let height = 1;
    let used = 60;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let new_gas_price = 200;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, new_gas_price)
        .unwrap();

    // then
    let expected_change =
        starting_exec_gas_price * exec_gas_price_increase_percent as u64 / 100;
    let expected = starting_exec_gas_price + expected_change;
    let actual = updater.new_scaled_exec_price;
    assert_eq!(actual, expected);
}
#[test]
fn update_l2_block_data__exec_price_will_not_go_below_min() {
    // given
    let starting_exec_gas_price = 100;
    let min_exec_gas_price = 50;
    let exec_gas_price_decrease_percent = 100;
    let threshold = 50;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_min_exec_gas_price(min_exec_gas_price)
        .with_exec_gas_price_change_percent(exec_gas_price_decrease_percent)
        .with_l2_block_capacity_threshold(threshold)
        .build();

    let height = 1;
    let used = 40;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let new_gas_price = 200;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, new_gas_price)
        .unwrap();

    // then
    let expected = min_exec_gas_price;
    let actual = updater.new_scaled_exec_price;
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__updates_last_and_last_last_profit() {
    // given
    let last_last_profit = 0;
    let total_cost = 500;
    let total_rewards = 1000;
    let last_profit = 200;
    let mut updater = UpdaterBuilder::new()
        .with_last_profit(last_profit, last_last_profit)
        .with_total_rewards(total_rewards)
        .with_projected_total_cost(total_cost)
        .build();

    let height = 1;
    let used = 50;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let new_gas_price = 100;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, new_gas_price)
        .unwrap();

    //  then
    let expected = last_profit;
    let actual = updater.second_to_last_profit;
    assert_eq!(actual, expected);
    // and
    let expected = total_rewards as i128 - total_cost as i128;
    let actual = updater.last_profit;
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__positive_profit_decrease_gas_price() {
    // given
    let starting_exec_gas_price = 100;
    let last_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 0; // DA is free
    let da_p_component = 100;
    let da_d_component = 10;
    let block_bytes = 500u64;
    let last_profit = i128::MAX;
    let last_last_profit = 0;
    let large_reward = i128::MAX;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_total_rewards(large_reward as u128)
        .with_known_total_cost(starting_cost as u128)
        .with_projected_total_cost(starting_cost as u128)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .with_last_profit(last_profit, last_last_profit)
        .with_da_max_change_percent(u16::MAX)
        .with_exec_gas_price_change_percent(0)
        .build();
    let old_gas_price = updater.algorithm().calculate();

    // when
    updater
        .update_l2_block_data(
            updater.l2_block_height + 1,
            50,
            100.try_into().unwrap(),
            block_bytes,
            200,
        )
        .unwrap();

    // then
    let new_gas_price = updater.algorithm().calculate();
    assert!(
        new_gas_price < old_gas_price,
        "{} !< {}",
        old_gas_price,
        new_gas_price
    );
}

#[test]
fn update_l2_block_data__price_does_not_decrease_more_than_max_percent() {
    // given
    let starting_exec_gas_price = 100;
    let starting_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 0; // DA is free
    let da_p_component = 100;
    let da_d_component = 10;
    let last_profit = i128::MAX; // Large, positive profit to decrease da price
    let last_last_profit = 0;
    let max_da_change_percent = 5;
    let large_starting_reward = i128::MAX;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_starting_da_gas_price(starting_da_gas_price)
        .with_total_rewards(large_starting_reward as u128)
        .with_known_total_cost(starting_cost as u128)
        .with_projected_total_cost(starting_cost as u128)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .with_last_profit(last_profit, last_last_profit)
        .with_da_max_change_percent(max_da_change_percent)
        .build();

    // when
    let height = updater.l2_block_height + 1;
    let used = 50;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let new_gas_price = 200;
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, new_gas_price)
        .unwrap();

    // then
    let algo = updater.algorithm();
    let actual = algo.calculate();
    let max_change =
        (starting_da_gas_price as f64 * max_da_change_percent as f64 / 100.0) as i64;
    let expected =
        starting_exec_gas_price as i64 + starting_da_gas_price as i64 - max_change;
    assert_eq!(expected as u64, actual);
}

#[test]
fn update_l2_block_data__da_price_does_not_increase_more_than_max_percent() {
    // given
    let starting_exec_gas_price = 100;
    let starting_da_gas_price = 100;
    let starting_cost = u128::MAX;
    let latest_gas_per_byte = u128::MAX; // DA is very expensive
    let da_p_component = 100;
    let da_d_component = 10;
    let last_profit = i128::MIN; // Large, negative profit to increase da price
    let last_last_profit = 0;
    let max_da_change_percent = 5;
    let large_starting_reward = 0;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_starting_da_gas_price(starting_da_gas_price)
        .with_total_rewards(large_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .with_last_profit(last_profit, last_last_profit)
        .with_da_max_change_percent(max_da_change_percent)
        .build();

    // when
    let height = updater.l2_block_height + 1;
    let used = 50;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let new_gas_price = 200;
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, new_gas_price)
        .unwrap();

    // then
    let algo = updater.algorithm();
    let actual = algo.calculate();
    let max_change =
        (starting_da_gas_price as f64 * max_da_change_percent as f64 / 100.0) as i64;
    let expected =
        starting_exec_gas_price as i64 + starting_da_gas_price as i64 + max_change;
    assert_eq!(expected as u64, actual);
}

#[test]
fn update_l2_block_data__never_drops_below_minimum_da_gas_price() {
    // given
    let starting_exec_gas_price = 0;
    let last_da_gas_price = 100;
    let min_da_gas_price = 100;
    let starting_cost = 0;
    let latest_gas_per_byte = 0; // DA is free
    let da_p_component = 100;
    let da_d_component = 10;
    let last_profit = i128::MAX;
    let avg_window = 10;
    let large_reward = u128::MAX;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_min_exec_gas_price(starting_exec_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_total_rewards(large_reward)
        .with_known_total_cost(starting_cost as u128)
        .with_projected_total_cost(starting_cost as u128)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .with_last_profit(last_profit, avg_window)
        .with_min_da_gas_price(min_da_gas_price)
        .build();

    // when
    updater
        .update_l2_block_data(
            updater.l2_block_height + 1,
            50,
            100.try_into().unwrap(),
            1000,
            200,
        )
        .unwrap();

    // then
    let algo = updater.algorithm();
    let actual = algo.calculate();
    let expected = min_da_gas_price;
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__even_profit_maintains_price() {
    // given
    let starting_exec_gas_price = 100;
    let starting_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_gas_price_denominator = 1;
    let block_bytes = 500u64;
    let starting_reward = starting_cost;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(starting_da_gas_price)
        .with_da_p_component(da_gas_price_denominator)
        .with_total_rewards(starting_reward as u128)
        .with_known_total_cost(starting_cost as u128)
        .with_projected_total_cost(starting_cost as u128)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .build();

    // when
    updater
        .update_l2_block_data(
            updater.l2_block_height + 1,
            50,
            100.try_into().unwrap(),
            block_bytes,
            starting_exec_gas_price + starting_da_gas_price,
        )
        .unwrap();
    let algo = updater.algorithm();
    let actual = algo.calculate();

    // then
    let expected = starting_exec_gas_price + starting_da_gas_price;
    assert_eq!(expected, actual);
}

#[test]
fn update_l2_block_data__negative_profit_increase_gas_price() {
    // given
    let starting_exec_gas_price = 100;
    let starting_da_gas_price = 100;
    let starting_cost = u128::MAX;
    let latest_gas_per_byte = i32::MAX; // DA is very expensive
    let da_p_component = 100;
    let da_d_component = 10;
    let last_profit = i128::MIN;
    let last_last_profit = 0;
    let smaller_starting_reward = 0;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(starting_da_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_total_rewards(smaller_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .with_last_profit(last_profit, last_last_profit)
        .build();
    let algo = updater.algorithm();
    let old_gas_price = algo.calculate();

    // when
    let height = updater.l2_block_height + 1;
    let used = 50;
    let capacity = 100u64.try_into().unwrap();
    let block_bytes = 500u64;
    let gas_price = 10;
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, gas_price)
        .unwrap();

    // then
    let algo = updater.algorithm();
    let new_gas_price = algo.calculate();
    assert!(
        new_gas_price > old_gas_price,
        "{} !> {}",
        new_gas_price,
        old_gas_price
    );
}
