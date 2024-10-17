use crate::v1::{
    tests::{
        BlockBytes,
        UpdaterBuilder,
    },
    Error,
    L2ActivityTracker,
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
    let fee = 100;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
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
    let fee = 100;

    // when
    let actual_error = updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
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
    let fee = 100;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
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
    let fee = 10_000;

    // when
    updater
        .update_l2_block_data(height, gas_used, capacity, block_bytes, fee)
        .unwrap();

    // then
    let expected = (fee * starting_da_gas_price as u128)
        .div_ceil(starting_da_gas_price as u128 + starting_exec_gas_price as u128);
    let actual = updater.total_da_rewards_excess;
    assert_eq!(actual, expected);
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
    let fee = 200;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
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
    let fee = 200;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
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
    let fee = 200;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
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
    let fee = 200;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
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
    let fee = 0; // No fee so it's easier to calculate profit

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
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

fn positive_profit_updater_builder() -> UpdaterBuilder {
    let starting_exec_gas_price = 100;
    let last_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 0; // DA is free
    let da_p_component = 100;
    let da_d_component = 10;
    let last_profit = i128::MAX;
    let last_last_profit = 0;
    let large_reward = i128::MAX;
    UpdaterBuilder::new()
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
}

#[test]
fn update_l2_block_data__positive_profit_decrease_gas_price() {
    // given
    let mut updater = positive_profit_updater_builder().build();
    let old_gas_price = updater.algorithm().calculate();

    // when
    let block_bytes = 500u64;
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
    let fee = 200;
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
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
    let fee = 200;
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
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
    let fee = 200;
    updater
        .update_l2_block_data(
            updater.l2_block_height + 1,
            50,
            100.try_into().unwrap(),
            1000,
            fee,
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
    let latest_cost_per_byte = 10;
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
        .with_da_cost_per_byte(latest_cost_per_byte as u128)
        .build();

    // when
    let da_fee = latest_cost_per_byte * block_bytes;
    let total_fee = da_fee * (starting_da_gas_price + starting_exec_gas_price)
        / starting_da_gas_price;
    updater
        .update_l2_block_data(
            updater.l2_block_height + 1,
            50,
            100.try_into().unwrap(),
            block_bytes,
            total_fee.into(),
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
    let mut updater = negative_profit_updater_builder().build();
    let algo = updater.algorithm();
    let old_gas_price = algo.calculate();

    // when
    let height = updater.l2_block_height + 1;
    let used = 50;
    let capacity = 100u64.try_into().unwrap();
    let block_bytes = 500u64;
    let fee = 0;
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
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

#[test]
fn update_l2_block_data__adds_l2_block_to_unrecorded_blocks() {
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
    let block_bytes = BlockBytes {
        height,
        block_bytes,
    };
    let expected = block_bytes.block_bytes;
    let actual = updater.unrecorded_blocks.get(&block_bytes.height).unwrap();
    assert_eq!(expected, *actual);
}

#[test]
fn update_l2_block_data__retains_existing_blocks_and_adds_l2_block_to_unrecorded_blocks()
{
    // given
    let starting_block = 0;
    let preexisting_block = BlockBytes {
        height: 0,
        block_bytes: 1000,
    };

    let mut updater = UpdaterBuilder::new()
        .with_l2_block_height(starting_block)
        .with_unrecorded_blocks(vec![preexisting_block.clone()])
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
    let block_bytes = BlockBytes {
        height,
        block_bytes,
    };
    let contains_block_bytes =
        updater.unrecorded_blocks.contains_key(&block_bytes.height);
    assert!(contains_block_bytes);

    let contains_preexisting_block_bytes = updater
        .unrecorded_blocks
        .contains_key(&preexisting_block.height);
    assert!(contains_preexisting_block_bytes);
}

fn hold_l2_activity() -> L2ActivityTracker {
    let increase = 1;
    let hold = 100;
    let decrease = 1;
    let activity = 50;
    let threshold = 50.into();
    L2ActivityTracker::new(increase, hold, decrease, activity, threshold)
}

#[test]
fn update_l2_block_data__da_gas_price_wants_to_increase_will_hold_if_activity_in_hold_range(
) {
    // given
    let hold_activity = hold_l2_activity();
    let mut updater = negative_profit_updater_builder()
        .with_activity(hold_activity)
        .build();
    let algo = updater.algorithm();
    let old_gas_price = algo.calculate();

    // when
    let height = updater.l2_block_height + 1;
    let used = 50;
    let capacity = 100u64.try_into().unwrap();
    let block_bytes = 500u64;
    let fee = 0;
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
        .unwrap();

    // then
    let algo = updater.algorithm();
    let new_gas_price = algo.calculate();
    assert_eq!(new_gas_price, old_gas_price,);
}

#[test]
fn update_l2_block_data__da_gas_price_wants_to_decrease_will_decrease_if_activity_in_hold_range(
) {
    // given
    let hold_activity = hold_l2_activity();
    let mut updater = positive_profit_updater_builder()
        .with_activity(hold_activity)
        .build();
    let old_gas_price = updater.algorithm().calculate();

    // when
    let block_bytes = 500u64;
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

fn decrease_l2_activity() -> L2ActivityTracker {
    let increase = 1;
    let hold = 1;
    let decrease = 100;
    let activity = 50;
    let threshold = 50.into();
    L2ActivityTracker::new(increase, hold, decrease, activity, threshold)
}

fn negative_profit_updater_builder() -> UpdaterBuilder {
    let starting_exec_gas_price = 100;
    let starting_da_gas_price = 100;
    let starting_cost = u128::MAX;
    let latest_gas_per_byte = i32::MAX; // DA is very expensive
    let da_p_component = 100;
    let da_d_component = 10;
    let last_profit = i128::MIN;
    let last_last_profit = 0;
    let smaller_starting_reward = 0;
    UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(starting_da_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_total_rewards(smaller_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .with_last_profit(last_profit, last_last_profit)
}
#[test]
fn update_l2_block_data__da_gas_price_wants_to_increase_will_decrease_if_activity_in_decrease_range(
) {
    // given
    let decrease_activity = decrease_l2_activity();
    let mut updater = negative_profit_updater_builder()
        .with_activity(decrease_activity)
        .build();
    let algo = updater.algorithm();
    let old_gas_price = algo.calculate();

    // when
    let height = updater.l2_block_height + 1;
    let used = 50;
    let capacity = 100u64.try_into().unwrap();
    let block_bytes = 500u64;
    let fee = 0;
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
        .unwrap();

    // then
    let algo = updater.algorithm();
    let new_gas_price = algo.calculate();
    assert!(
        new_gas_price < old_gas_price,
        "{} !> {}",
        new_gas_price,
        old_gas_price
    );
}

#[test]
fn update_l2_block_data__da_gas_price_wants_to_decrease_will_decrease_if_activity_in_decrease_range(
) {
    // given
    let decrease_activity = decrease_l2_activity();
    let mut updater = positive_profit_updater_builder()
        .with_activity(decrease_activity)
        .build();
    let old_gas_price = updater.algorithm().calculate();

    // when
    let block_bytes = 500u64;
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
fn update_l2_block_data__above_threshold_increase_activity() {
    // given
    let starting_exec_gas_price = 100;
    let exec_gas_price_increase_percent = 10;
    let threshold = 50;
    let starting_activity = 2;
    let activity = L2ActivityTracker::new(1, 1, 1, starting_activity, 50.into());
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_exec_gas_price_change_percent(exec_gas_price_increase_percent)
        .with_l2_block_capacity_threshold(threshold)
        .with_activity(activity)
        .build();

    let height = 1;
    let used = 60;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let fee = 200;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
        .unwrap();

    // then
    let expected = starting_activity + 1;
    let actual = updater.l2_activity.current_activity();
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__below_threshold_decrease_activity() {
    // given
    let starting_exec_gas_price = 100;
    let exec_gas_price_increase_percent = 10;
    let threshold = 50;
    let starting_activity = 2;
    let activity = L2ActivityTracker::new(1, 1, 1, starting_activity, 50.into());
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_exec_gas_price_change_percent(exec_gas_price_increase_percent)
        .with_l2_block_capacity_threshold(threshold)
        .with_activity(activity)
        .build();

    let height = 1;
    let used = 40;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let fee = 200;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
        .unwrap();

    // then
    let expected = starting_activity - 1;
    let actual = updater.l2_activity.current_activity();
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__if_activity_at_max_will_stop_increasing() {
    // given
    let starting_exec_gas_price = 100;
    let exec_gas_price_increase_percent = 10;
    let threshold = 50;
    let increase_range = 1;
    let hold_range = 1;
    let decrease_range = 1;
    let starting_activity = increase_range + hold_range + decrease_range;
    let activity = L2ActivityTracker::new(
        increase_range,
        hold_range,
        decrease_range,
        starting_activity,
        50.into(),
    );
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_exec_gas_price_change_percent(exec_gas_price_increase_percent)
        .with_l2_block_capacity_threshold(threshold)
        .with_activity(activity)
        .build();

    let height = 1;
    let used = 60;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let fee = 200;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
        .unwrap();

    // then
    let expected = starting_activity;
    let actual = updater.l2_activity.current_activity();
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__if_activity_is_zero_will_stop_decreasing() {
    // given
    let starting_exec_gas_price = 100;
    let exec_gas_price_increase_percent = 10;
    let threshold = 50;
    let starting_activity = 0;
    let activity = L2ActivityTracker::new(1, 1, 1, starting_activity, 50.into());
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_exec_gas_price_change_percent(exec_gas_price_increase_percent)
        .with_l2_block_capacity_threshold(threshold)
        .with_activity(activity)
        .build();

    let height = 1;
    let used = 40;
    let capacity = 100.try_into().unwrap();
    let block_bytes = 1000;
    let fee = 200;

    // when
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, fee)
        .unwrap();

    // then
    let expected = starting_activity;
    let actual = updater.l2_activity.current_activity();
    assert_eq!(actual, expected);
}
