use crate::v0::{
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

    // when
    updater
        .update_l2_block_data(height, used, capacity)
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

    // when
    let actual_error = updater
        .update_l2_block_data(height, used, capacity)
        .unwrap_err();

    // then
    let expected_error = Error::SkippedL2Block {
        expected: starting_block + 1,
        got: 2,
    };
    assert_eq!(actual_error, expected_error);
}

#[test]
fn update_l2_block_data__even_threshold_will_increase_exec_gas_price() {
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

    // when
    updater
        .update_l2_block_data(height, used, capacity)
        .unwrap();

    // then
    let expected = starting_gas_price + starting_gas_price * unused_percent / 100;
    let actual = updater.new_exec_price;
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

    // when
    updater
        .update_l2_block_data(height, used, capacity)
        .unwrap();

    // then
    let expected_change_amount =
        starting_exec_gas_price * exec_gas_price_decrease_percent / 100;
    let expected = starting_exec_gas_price - expected_change_amount;
    let actual = updater.new_exec_price;
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

    // when
    updater
        .update_l2_block_data(height, used, capacity)
        .unwrap();

    // then
    let expected_change = starting_exec_gas_price * exec_gas_price_increase_percent / 100;
    let expected = starting_exec_gas_price + expected_change;
    let actual = updater.new_exec_price;
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

    // when
    updater
        .update_l2_block_data(height, used, capacity)
        .unwrap();

    // then
    let expected = min_exec_gas_price;
    let actual = updater.new_exec_price;
    assert_eq!(actual, expected);
}
