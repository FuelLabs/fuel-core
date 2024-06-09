use super::*;

#[test]
fn update_l2_block_data__updates_l2_block() {
    // given
    let starting_block = 0;

    let mut updater = UpdaterBuilder::new()
        .with_l2_block_height(starting_block)
        .build();

    let height = 1;
    let block_reward = 100;
    let block_bytes = 1000;


    // when
    updater.update_l2_block_data(height, block_reward, block_bytes).unwrap();

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
    let block_reward = 100;
    let block_bytes = 1000;

    // when
    let actual_error = updater.update_l2_block_data(height, block_reward, block_bytes).unwrap_err();

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
    let block_reward = 100;
    let block_bytes = 1000;

    // when
    updater.update_l2_block_data(height, block_reward, block_bytes).unwrap();

    // then
    let expected = block_bytes * da_cost_per_byte;
    let actual = updater.projected_total_cost;
    assert_eq!(actual, expected);
}
