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
    let expected_new_da_price = new_gas_price - starting_exec_gas_price;
    let expected = gas_used * expected_new_da_price;
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
fn update_l2_block_data__updates_last_da_gas_price() {
    // given

    let starting_exec_gas_price = 100;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
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
    let expected = new_gas_price - starting_exec_gas_price;
    let actual = updater.last_da_gas_price;
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
