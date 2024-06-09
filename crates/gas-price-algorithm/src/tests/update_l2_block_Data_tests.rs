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


    // when
    updater.update_l2_block_data(height, fullness, block_bytes).unwrap();

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

    // when
    let actual_error = updater.update_l2_block_data(height, fullness, block_bytes).unwrap_err();

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

    // when
    updater.update_l2_block_data(height, fullness, block_bytes).unwrap();

    // then
    let expected = block_bytes * da_cost_per_byte;
    let actual = updater.projected_total_cost;
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_date__if_above_capacity_threshold_increase_price() {
    // given
    let starting_gas_price = 100;
    let threshold = 50;
    let increase_amount = 10;
    let mut updater = UpdaterBuilder::new()
        .with_starting_gas_price(starting_gas_price)
        .with_l2_block_capacity_threshold(threshold)
        .with_l2_comp_gas_price_increase_amount(increase_amount)
        .build();

    let height = 1;
    let fullness = (60, 100);
    let block_bytes = 1000;

    // when
    updater.update_l2_block_data(height, fullness, block_bytes).unwrap();

    // then
    let expected = starting_gas_price + increase_amount;
    let actual = updater.gas_price();
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__if_below_capacity_threshold_decrease_price() {
    // given
    let starting_gas_price = 100;
    let threshold = 50;
    let increase_amount = 10;
    let mut updater = UpdaterBuilder::new()
        .with_starting_gas_price(starting_gas_price)
        .with_l2_block_capacity_threshold(threshold)
        .with_l2_comp_gas_price_increase_amount(increase_amount)
        .build();

    let height = 1;
    let fullness = (40, 100);
    let block_bytes = 1000;

    // when
    updater.update_l2_block_data(height, fullness, block_bytes).unwrap();

    // then
    let expected = starting_gas_price;
    let actual = updater.gas_price();
    assert_eq!(actual, expected);
}

#[test]
fn update_l2_block_data__updates_the_total_reward_value() {
    // given
    let starting_gas_price = 100;
    let mut updater = UpdaterBuilder::new()
        .with_starting_gas_price(starting_gas_price)
        .build();

    let height = 1;
    let gas_used = 50;
    let fullness = (gas_used, 100);
    let block_bytes = 1000;

    // when
    updater.update_l2_block_data(height, fullness, block_bytes).unwrap();

    // then
    let expected = gas_used * starting_gas_price;
    let actual = updater.total_rewards;
    assert_eq!(actual, expected);
}
