use crate::v1::{
    tests::{
        BlockBytes,
        UpdaterBuilder,
    },
    Error,
};

#[test]
fn update_da_record_data__throws_error_if_receives_a_block_missing_from_unrecorded_blocks(
) {
    // given
    let recorded_range = (1u32..3).collect();
    let recorded_cost = 200;
    let unrecorded_blocks = vec![BlockBytes {
        height: 1,
        block_bytes: 1000,
    }];
    let mut updater = UpdaterBuilder::new()
        .with_unrecorded_blocks(unrecorded_blocks)
        .build();

    // when
    let actual_error = updater
        .update_da_record_data(recorded_range, recorded_cost)
        .unwrap_err();

    // then
    let expected_error = Error::L2BlockExpectedNotFound { height: 2 };
    assert_eq!(actual_error, expected_error);
}

#[test]
fn update_da_record_data__updates_cost_per_byte() {
    // given
    let da_cost_per_byte = 20;
    let block_bytes = 1000;
    let unrecorded_blocks = vec![BlockBytes {
        height: 1,
        block_bytes,
    }];
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_unrecorded_blocks(unrecorded_blocks)
        .build();

    let new_cost_per_byte = 100;
    let recorded_cost = (block_bytes * new_cost_per_byte) as u128;
    let recorded_range = (1u32..2).collect();
    // when
    updater
        .update_da_record_data(recorded_range, recorded_cost)
        .unwrap();

    // then
    let expected = new_cost_per_byte as u128;
    let actual = updater.latest_da_cost_per_byte;
    assert_eq!(actual, expected);
}

#[test]
fn update_da_record_data__updates_known_total_cost() {
    // given
    let da_cost_per_byte = 20;
    let l2_block_height = 15;
    let projected_total_cost = 2000;
    let known_total_cost = 1500;
    let unrecorded_blocks = vec![
        BlockBytes {
            height: 11,
            block_bytes: 1000,
        },
        BlockBytes {
            height: 12,
            block_bytes: 2000,
        },
        BlockBytes {
            height: 13,
            block_bytes: 1500,
        },
    ];
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost)
        .with_known_total_cost(known_total_cost)
        .with_unrecorded_blocks(unrecorded_blocks)
        .build();

    let recorded_range = (11u32..14).collect();
    let recorded_cost = 300;
    // when
    updater
        .update_da_record_data(recorded_range, recorded_cost)
        .unwrap();

    // then
    let actual = updater.latest_known_total_da_cost_excess;
    let expected = known_total_cost + recorded_cost;
    assert_eq!(actual, expected);
}

#[test]
fn update_da_record_data__if_da_height_matches_l2_height_projected_and_known_match() {
    // given
    let da_cost_per_byte = 20;
    let l2_block_height = 13;
    let known_total_cost = 1500;
    let unrecorded_blocks = vec![
        BlockBytes {
            height: 11,
            block_bytes: 1000,
        },
        BlockBytes {
            height: 12,
            block_bytes: 2000,
        },
        BlockBytes {
            height: 13,
            block_bytes: 1500,
        },
    ];

    let guessed_cost: u64 = unrecorded_blocks
        .iter()
        .map(|block| block.block_bytes * da_cost_per_byte)
        .sum();
    let projected_total_cost = known_total_cost + guessed_cost;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte as u128)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost as u128)
        .with_known_total_cost(known_total_cost as u128)
        .with_unrecorded_blocks(unrecorded_blocks)
        .build();

    let block_bytes = 1000;
    let new_cost_per_byte = 100;
    let block_cost = block_bytes * new_cost_per_byte;

    let recorded_range = (11u32..14).collect();
    let recorded_cost = block_cost * 3;
    // when
    updater
        .update_da_record_data(recorded_range, recorded_cost)
        .unwrap();

    // then
    assert_eq!(updater.unrecorded_blocks.len(), 0);
    assert_eq!(
        updater.projected_total_da_cost,
        updater.latest_known_total_da_cost_excess
    );
}

#[test]
fn update_da_record_data__da_block_updates_projected_total_cost_with_known_and_guesses_on_top(
) {
    // given
    let da_cost_per_byte = 20;
    let l2_block_height = 15;
    let original_known_total_cost = 1500;
    let block_bytes = 1000;
    let mut unrecorded_blocks = vec![
        BlockBytes {
            height: 11,
            block_bytes: 1000,
        },
        BlockBytes {
            height: 12,
            block_bytes: 2000,
        },
        BlockBytes {
            height: 13,
            block_bytes: 1500,
        },
    ];

    let remaining = vec![
        BlockBytes {
            height: 14,
            block_bytes,
        },
        BlockBytes {
            height: 15,
            block_bytes,
        },
    ];
    let to_be_removed = unrecorded_blocks.clone();
    unrecorded_blocks.extend(remaining.clone());
    let guessed_cost: u64 = unrecorded_blocks
        .iter()
        .map(|block| block.block_bytes * da_cost_per_byte)
        .sum();
    let projected_total_cost = original_known_total_cost + guessed_cost;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte as u128)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost as u128)
        .with_known_total_cost(original_known_total_cost as u128)
        .with_unrecorded_blocks(unrecorded_blocks)
        .build();

    let new_cost_per_byte = 100;
    let (recorded_heights, recorded_cost) =
        to_be_removed
            .iter()
            .fold((vec![], 0), |(mut range, cost), block| {
                range.push(block.height);
                (range, cost + block.block_bytes * new_cost_per_byte)
            });
    let min = recorded_heights.iter().min().unwrap();
    let max = recorded_heights.iter().max().unwrap();
    let recorded_range = (*min..(max + 1)).collect();

    // when
    updater
        .update_da_record_data(recorded_range, recorded_cost as u128)
        .unwrap();

    // then
    let actual = updater.projected_total_da_cost;
    let new_known_total_cost = original_known_total_cost + recorded_cost;
    let guessed_part: u64 = remaining
        .iter()
        .map(|block| block.block_bytes * new_cost_per_byte)
        .sum();
    let expected = new_known_total_cost + guessed_part;
    assert_eq!(actual, expected as u128);
}

#[test]
fn update_da_record_data__updates_known_total_cost_if_blocks_are_out_of_order() {
    // given
    let da_cost_per_byte = 20;
    let block_bytes = 1000;
    let unrecorded_blocks = vec![
        BlockBytes {
            height: 1,
            block_bytes,
        },
        BlockBytes {
            height: 2,
            block_bytes,
        },
        BlockBytes {
            height: 3,
            block_bytes,
        },
    ];
    let old_known_total_cost = 500;
    let old_projected_total_cost =
        old_known_total_cost + (block_bytes as u128 * da_cost_per_byte * 3);
    let old_da_cost_per_byte = 20;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_unrecorded_blocks(unrecorded_blocks)
        .with_da_cost_per_byte(old_da_cost_per_byte)
        .with_known_total_cost(old_known_total_cost)
        .with_projected_total_cost(old_projected_total_cost)
        .build();
    let new_cost_per_byte = 100;
    let recorded_cost = 2 * (block_bytes * new_cost_per_byte) as u128;
    let recorded_range = vec![2, 3];

    // when
    updater
        .update_da_record_data(recorded_range, recorded_cost)
        .unwrap();

    // and
    let expected = updater.latest_known_total_da_cost_excess
        + (block_bytes * new_cost_per_byte) as u128;
    let actual = updater.projected_total_da_cost;
    assert_eq!(actual, expected);
}
#[test]
fn update_da_record_data__updates_projected_total_cost_if_blocks_are_out_of_order() {
    // given
    let da_cost_per_byte = 20;
    let block_bytes = 1000;
    let unrecorded_blocks = vec![
        BlockBytes {
            height: 1,
            block_bytes,
        },
        BlockBytes {
            height: 2,
            block_bytes,
        },
        BlockBytes {
            height: 3,
            block_bytes,
        },
    ];
    let old_known_total_cost = 500;
    let old_projected_total_cost =
        old_known_total_cost + (block_bytes as u128 * da_cost_per_byte * 3);
    let old_da_cost_per_byte = 20;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_unrecorded_blocks(unrecorded_blocks)
        .with_da_cost_per_byte(old_da_cost_per_byte)
        .with_known_total_cost(old_known_total_cost)
        .with_projected_total_cost(old_projected_total_cost)
        .build();
    let new_cost_per_byte = 100;
    let recorded_cost = 2 * (block_bytes * new_cost_per_byte) as u128;
    let recorded_range = vec![2, 3];

    // when
    updater
        .update_da_record_data(recorded_range, recorded_cost)
        .unwrap();

    // then
    let expected = updater.latest_known_total_da_cost_excess
        + (block_bytes * new_cost_per_byte) as u128;
    let actual = updater.projected_total_da_cost;
    assert_eq!(actual, expected);
}

#[test]
fn update_da_record_data__updates_unrecorded_blocks() {
    // given
    let da_cost_per_byte = 20;
    let block_bytes = 1000;
    let unrecorded_blocks = vec![
        BlockBytes {
            height: 1,
            block_bytes,
        },
        BlockBytes {
            height: 2,
            block_bytes,
        },
        BlockBytes {
            height: 3,
            block_bytes,
        },
    ];
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_unrecorded_blocks(unrecorded_blocks)
        .build();
    let new_cost_per_byte = 100;
    let recorded_cost = 2 * (block_bytes * new_cost_per_byte) as u128;
    let recorded_range = vec![2, 3];

    // when
    updater
        .update_da_record_data(recorded_range, recorded_cost)
        .unwrap();

    // then
    let expected = vec![(1, block_bytes)];
    let actual: Vec<_> = updater.unrecorded_blocks.into_iter().collect();
    assert_eq!(actual, expected);
}
