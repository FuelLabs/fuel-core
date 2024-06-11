use super::*;

#[test]
fn update_l2_block_data__updates_l2_block() {
    // given
    let starting_block = 0;

    let mut updater = UpdaterBuilder::new()
        .with_l2_block_height(starting_block)
        .build();

    let height = 1;
    let fullness = (50, 100);
    let block_bytes = 1000;
    let new_gas_price = 100;

    // when
    updater
        .update_l2_block_data(height, fullness, block_bytes, new_gas_price)
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
    let fullness = (50, 100);
    let block_bytes = 1000;
    let new_gas_price = 100;

    // when
    let actual_error = updater
        .update_l2_block_data(height, fullness, block_bytes, new_gas_price)
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
    let fullness = (50, 100);
    let block_bytes = 1000;
    let new_gas_price = 100;

    // when
    updater
        .update_l2_block_data(height, fullness, block_bytes, new_gas_price)
        .unwrap();

    // then
    let expected = block_bytes * da_cost_per_byte;
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
    let fullness = (gas_used, 100);
    let block_bytes = 1000;
    let new_gas_price = 200;

    // when
    updater
        .update_l2_block_data(height, fullness, block_bytes, new_gas_price)
        .unwrap();

    // then
    let expected_new_da_price = new_gas_price - starting_exec_gas_price;
    let expected = gas_used * expected_new_da_price;
    let actual = updater.total_da_rewards;
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__even_threshold_will_not_change_exec_gas_price() {
    // given
    let starting_gas_price = 100;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_gas_price)
        .build();

    let height = 1;
    let fullness = (50, 100);
    let block_bytes = 1000;
    let new_gas_price = 200;

    // when
    updater
        .update_l2_block_data(height, fullness, block_bytes, new_gas_price)
        .unwrap();

    // then
    let expected = starting_gas_price;
    let actual = updater.new_exec_price;
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__below_threshold_will_decrease_exec_gas_price() {
    // given
    let starting_exec_gas_price = 100;
    let exec_gas_price_increase_amount = 10;
    let threshold = 50;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_exec_gas_price_increase_amount(exec_gas_price_increase_amount)
        .with_l2_block_capacity_threshold(threshold)
        .build();

    let height = 1;
    let fullness = (40, 100);
    let block_bytes = 1000;
    let new_gas_price = 200;

    // when
    updater
        .update_l2_block_data(height, fullness, block_bytes, new_gas_price)
        .unwrap();

    // then
    let expected = starting_exec_gas_price - exec_gas_price_increase_amount;
    let actual = updater.new_exec_price;
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__above_threshold_will_increase_exec_gas_price() {
    // given
    let starting_exec_gas_price = 100;
    let exec_gas_price_increase_amount = 10;
    let threshold = 50;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_exec_gas_price_increase_amount(exec_gas_price_increase_amount)
        .with_l2_block_capacity_threshold(threshold)
        .build();

    let height = 1;
    let fullness = (60, 100);
    let block_bytes = 1000;
    let new_gas_price = 200;

    // when
    updater
        .update_l2_block_data(height, fullness, block_bytes, new_gas_price)
        .unwrap();

    // then
    let expected = starting_exec_gas_price + exec_gas_price_increase_amount;
    let actual = updater.new_exec_price;
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
    let fullness = (50, 100);
    let block_bytes = 1000;
    let new_gas_price = 200;

    // when
    updater
        .update_l2_block_data(height, fullness, block_bytes, new_gas_price)
        .unwrap();

    // then
    let expected = new_gas_price - starting_exec_gas_price;
    let actual = updater.last_da_price;
    assert_eq!(actual, expected);
}
