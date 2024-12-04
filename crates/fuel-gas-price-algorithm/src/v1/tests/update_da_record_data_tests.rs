use crate::v1::tests::UpdaterBuilder;
use std::collections::BTreeMap;

#[test]
fn update_da_record_data__if_receives_batch_with_unknown_blocks_will_include_known_blocks_with_previous_cost(
) {
    // given
    let recorded_heights: Vec<u32> = (1u32..3).collect();
    let recorded_cost = 1_000_000;
    let recorded_bytes = 500;
    let block_bytes = 1000;
    let mut unrecorded_blocks: BTreeMap<_, _> = [(1, block_bytes)].into_iter().collect();
    let cost_per_byte = 333;
    let known_total_cost = 10_000;
    let mut updater = UpdaterBuilder::new()
        .with_unrecorded_blocks(&unrecorded_blocks)
        .with_da_cost_per_byte(cost_per_byte)
        .with_known_total_cost(known_total_cost)
        .build();
    let old = updater.algorithm();

    // when
    updater
        .update_da_record_data(
            &recorded_heights,
            recorded_bytes,
            recorded_cost,
            &mut unrecorded_blocks,
        )
        .unwrap();

    // then
    let new = updater.algorithm();
    assert_eq!(new, old);
    let expected = known_total_cost + recorded_cost;
    let actual = updater.latest_known_total_da_cost_excess;
    assert_eq!(expected, actual);
}

#[test]
fn update_da_record_data__if_receives_batch_with_unknown_blocks_will_never_increase_cost_more_than_recorded_cost(
) {
    // given
    let recorded_heights: Vec<u32> = (1u32..3).collect();
    let recorded_cost = 200;
    let block_bytes = 1000;
    let recorded_bytes = 500;
    let mut unrecorded_blocks: BTreeMap<_, _> = [(1, block_bytes)].into_iter().collect();
    let cost_per_byte = 333;
    let known_total_cost = 10_000;
    let mut updater = UpdaterBuilder::new()
        .with_unrecorded_blocks(&unrecorded_blocks)
        .with_da_cost_per_byte(cost_per_byte)
        .with_known_total_cost(known_total_cost)
        .build();
    let old = updater.algorithm();

    // when
    updater
        .update_da_record_data(
            &recorded_heights,
            recorded_bytes,
            recorded_cost,
            &mut unrecorded_blocks,
        )
        .unwrap();

    // then
    let new = updater.algorithm();
    assert_eq!(new, old);
    let expected = known_total_cost + recorded_cost;
    let actual = updater.latest_known_total_da_cost_excess;
    assert_eq!(expected, actual);
}

#[test]
fn update_da_record_data__updates_cost_per_byte() {
    // given
    let da_cost_per_byte = 20;
    let block_bytes = 1000;
    let mut unrecorded_blocks: BTreeMap<_, _> = [(1, block_bytes)].into_iter().collect();
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_unrecorded_blocks(&unrecorded_blocks)
        .build();

    let new_cost_per_byte = 100;
    let recorded_bytes = 500;
    let recorded_cost = (recorded_bytes * new_cost_per_byte) as u128;
    let recorded_heights: Vec<u32> = (1u32..2).collect();
    // when
    updater
        .update_da_record_data(
            &recorded_heights,
            recorded_bytes,
            recorded_cost,
            &mut unrecorded_blocks,
        )
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
    let mut unrecorded_blocks: BTreeMap<_, _> =
        [(11, 1000), (12, 2000), (13, 1500)].into_iter().collect();
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost)
        .with_known_total_cost(known_total_cost)
        .with_unrecorded_blocks(&unrecorded_blocks)
        .build();

    let recorded_heights: Vec<u32> = (11u32..14).collect();
    let recorded_bytes = 500;
    let recorded_cost = 300;
    // when
    updater
        .update_da_record_data(
            &recorded_heights,
            recorded_bytes,
            recorded_cost,
            &mut unrecorded_blocks,
        )
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
    let mut unrecorded_blocks: BTreeMap<_, _> =
        [(11, 1000), (12, 2000), (13, 1500)].into_iter().collect();

    let guessed_cost: u64 = unrecorded_blocks
        .values()
        .map(|bytes| bytes * da_cost_per_byte)
        .sum();
    let projected_total_cost = known_total_cost + guessed_cost;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte as u128)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost as u128)
        .with_known_total_cost(known_total_cost as u128)
        .with_unrecorded_blocks(&unrecorded_blocks)
        .build();

    let block_bytes = 1000;
    let new_cost_per_byte = 100;
    let block_cost = block_bytes * new_cost_per_byte;

    let recorded_heights: Vec<u32> = (11u32..14).collect();
    let recorded_bytes = 500;
    let recorded_cost = block_cost * 3;
    // when
    updater
        .update_da_record_data(
            &recorded_heights,
            recorded_bytes,
            recorded_cost,
            &mut unrecorded_blocks,
        )
        .unwrap();

    // then
    assert_eq!(unrecorded_blocks.len(), 0);
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
    let original_known_total_cost: u128 = 1500;
    let block_bytes = 1000;
    let remaining = vec![(14, block_bytes), (15, block_bytes)];
    let mut pairs = vec![(11, 1000), (12, 2000), (13, 1500)];

    pairs.extend(remaining.clone());

    let mut unrecorded_blocks: BTreeMap<_, _> = pairs.into_iter().collect();

    let guessed_cost: u128 = unrecorded_blocks
        .values()
        .map(|bytes| *bytes as u128 * da_cost_per_byte)
        .sum();
    let projected_total_cost: u128 = original_known_total_cost + guessed_cost;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost)
        .with_known_total_cost(original_known_total_cost)
        .with_unrecorded_blocks(&unrecorded_blocks)
        .build();

    let new_cost_per_byte = 100;
    let recorded_heights = vec![11, 12, 13];
    let recorded_bytes = 500;
    let recorded_cost = recorded_bytes * new_cost_per_byte;
    let recorded_bytes = 500;

    // when
    updater
        .update_da_record_data(
            &recorded_heights,
            recorded_bytes,
            recorded_cost,
            &mut unrecorded_blocks,
        )
        .unwrap();

    // then
    let actual = updater.projected_total_da_cost;
    let new_known_total_cost = original_known_total_cost + recorded_cost;
    let guessed_part: u128 = remaining
        .iter()
        .map(|(_, bytes)| *bytes as u128 * new_cost_per_byte)
        .sum();
    let expected = new_known_total_cost + guessed_part;
    assert_eq!(actual, expected);
}

#[test]
fn update_da_record_data__updates_known_total_cost_if_blocks_are_out_of_order() {
    // given
    let da_cost_per_byte = 20;
    let block_bytes = 1000;
    let mut unrecorded_blocks: BTreeMap<_, _> =
        [(1, block_bytes), (2, block_bytes), (3, block_bytes)]
            .into_iter()
            .collect();
    let old_known_total_cost = 500;
    let old_projected_total_cost =
        old_known_total_cost + (block_bytes as u128 * da_cost_per_byte * 3);
    let old_da_cost_per_byte = 20;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_unrecorded_blocks(&unrecorded_blocks)
        .with_da_cost_per_byte(old_da_cost_per_byte)
        .with_known_total_cost(old_known_total_cost)
        .with_projected_total_cost(old_projected_total_cost)
        .build();
    let new_cost_per_byte = 100;
    let recorded_bytes = 500;
    let recorded_cost = recorded_bytes * new_cost_per_byte;
    let recorded_heights: Vec<u32> = vec![3, 2];

    // when
    updater
        .update_da_record_data(
            &recorded_heights,
            recorded_bytes,
            recorded_cost as u128,
            &mut unrecorded_blocks,
        )
        .unwrap();

    // then
    let expected = updater.latest_known_total_da_cost_excess
        + (block_bytes as u128 * new_cost_per_byte as u128);
    let actual = updater.projected_total_da_cost;
    assert_eq!(actual, expected);
}

#[test]
fn update_da_record_data__updates_projected_total_cost_if_blocks_are_out_of_order() {
    // given
    let da_cost_per_byte = 20;
    let block_bytes = 1000;
    let mut unrecorded_blocks: BTreeMap<_, _> =
        [(1, block_bytes), (2, block_bytes), (3, block_bytes)]
            .into_iter()
            .collect();
    let old_known_total_cost = 500;
    let old_projected_total_cost =
        old_known_total_cost + (block_bytes as u128 * da_cost_per_byte * 3);
    let old_da_cost_per_byte = 20;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_unrecorded_blocks(&unrecorded_blocks)
        .with_da_cost_per_byte(old_da_cost_per_byte)
        .with_known_total_cost(old_known_total_cost)
        .with_projected_total_cost(old_projected_total_cost)
        .build();
    let new_cost_per_byte = 100;
    let recorded_bytes = 500;
    let recorded_cost = recorded_bytes * new_cost_per_byte;
    let recorded_heights: Vec<u32> = vec![3, 2];

    // when
    updater
        .update_da_record_data(
            &recorded_heights,
            recorded_bytes,
            recorded_cost as u128,
            &mut unrecorded_blocks,
        )
        .unwrap();

    // then
    let expected = updater.latest_known_total_da_cost_excess
        + (block_bytes as u128 * new_cost_per_byte as u128);
    let actual = updater.projected_total_da_cost;
    assert_eq!(actual, expected);
}

#[test]
fn update_da_record_data__updates_unrecorded_blocks() {
    // given
    let da_cost_per_byte = 20;
    let block_bytes = 1000;
    let mut unrecorded_blocks: BTreeMap<_, _> =
        [(1, block_bytes), (2, block_bytes), (3, block_bytes)]
            .into_iter()
            .collect();
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_unrecorded_blocks(&unrecorded_blocks)
        .build();
    let new_cost_per_byte = 100;
    let recorded_bytes = 500;
    let recorded_cost = 2 * (recorded_bytes * new_cost_per_byte) as u128;
    let recorded_heights: Vec<u32> = vec![3, 2];

    // when
    updater
        .update_da_record_data(
            &recorded_heights,
            recorded_bytes,
            recorded_cost,
            &mut unrecorded_blocks,
        )
        .unwrap();

    // then
    let expected = vec![(1, block_bytes)];
    let actual: Vec<_> = unrecorded_blocks.into_iter().collect();
    assert_eq!(actual, expected);
}

#[test]
fn update_da_record_data__da_block_lowers_da_gas_price() {
    // given
    let da_cost_per_byte = 40;
    let l2_block_height = 11;
    let original_known_total_cost = 150;
    // let unrecorded_blocks = vec![BlockBytes {
    //     height: 11,
    //     block_bytes: 3000,
    // }];
    let mut unrecorded_blocks: BTreeMap<_, _> = [(11, 3000)].into_iter().collect();
    let da_p_component = 2;
    let guessed_cost: u64 = unrecorded_blocks
        .values()
        .map(|bytes| bytes * da_cost_per_byte)
        .sum();
    let projected_total_cost = original_known_total_cost + guessed_cost;

    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte as u128)
        .with_da_p_component(da_p_component)
        .with_last_profit(10, 0)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost as u128)
        .with_known_total_cost(original_known_total_cost as u128)
        .with_unrecorded_blocks(&unrecorded_blocks)
        .build();

    let new_cost_per_byte = 100;
    let (recorded_heights, recorded_cost) = unrecorded_blocks.iter().fold(
        (vec![], 0),
        |(mut range, cost), (height, bytes)| {
            range.push(height);
            (range, cost + bytes * new_cost_per_byte)
        },
    );
    let min = *recorded_heights.iter().min().unwrap();
    let max = *recorded_heights.iter().max().unwrap();
    let recorded_range: Vec<u32> = (*min..(max + 1)).collect();
    let recorded_bytes = 500;

    let old_da_gas_price = updater.new_scaled_da_gas_price;

    // when
    updater
        .update_da_record_data(
            &recorded_range,
            recorded_bytes,
            recorded_cost as u128,
            &mut unrecorded_blocks,
        )
        .unwrap();

    // then
    let new_da_gas_price = updater.new_scaled_da_gas_price;
    // because the profit is 10 and the da_p_component is 2, the new da gas price should be lesser than the previous one.
    assert_eq!(new_da_gas_price, 0);
    assert_ne!(old_da_gas_price, new_da_gas_price);
}

#[test]
fn update_da_record_data__da_block_increases_da_gas_price() {
    // given
    let da_cost_per_byte = 40;
    let l2_block_height = 11;
    let original_known_total_cost = 150;
    let mut unrecorded_blocks: BTreeMap<_, _> = [(11, 3000)].into_iter().collect();
    let da_p_component = 2;
    let guessed_cost: u64 = unrecorded_blocks
        .values()
        .map(|bytes| bytes * da_cost_per_byte)
        .sum();
    let projected_total_cost = original_known_total_cost + guessed_cost;

    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte as u128)
        .with_da_p_component(da_p_component)
        .with_last_profit(-10, 0)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost as u128)
        .with_known_total_cost(original_known_total_cost as u128)
        .with_unrecorded_blocks(&unrecorded_blocks)
        .build();

    let new_cost_per_byte = 100;
    let (recorded_heights, recorded_cost) = unrecorded_blocks.iter().fold(
        (vec![], 0),
        |(mut range, cost), (height, bytes)| {
            range.push(height);
            (range, cost + bytes * new_cost_per_byte)
        },
    );

    let min = *recorded_heights.iter().min().unwrap();
    let max = *recorded_heights.iter().max().unwrap();
    let recorded_range: Vec<u32> = (*min..(max + 1)).collect();
    let recorded_bytes = 500;

    let old_da_gas_price = updater.new_scaled_da_gas_price;

    // when
    updater
        .update_da_record_data(
            &recorded_range,
            recorded_bytes,
            recorded_cost as u128,
            &mut unrecorded_blocks,
        )
        .unwrap();

    // then
    let new_da_gas_price = updater.new_scaled_da_gas_price;
    // because the profit is -10 and the da_p_component is 2, the new da gas price should be greater than the previous one.
    assert_eq!(new_da_gas_price, 6);
    assert_ne!(old_da_gas_price, new_da_gas_price);
}

#[test]
fn update_da_record_data__da_block_will_not_change_da_gas_price() {
    // given
    let da_cost_per_byte = 40;
    let l2_block_height = 11;
    let original_known_total_cost = 150;
    let mut unrecorded_blocks: BTreeMap<_, _> = [(11, 3000)].into_iter().collect();
    let da_p_component = 2;
    let guessed_cost: u64 = unrecorded_blocks
        .values()
        .map(|bytes| bytes * da_cost_per_byte)
        .sum();
    let projected_total_cost = original_known_total_cost + guessed_cost;

    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte as u128)
        .with_da_p_component(da_p_component)
        .with_last_profit(0, 0)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost as u128)
        .with_known_total_cost(original_known_total_cost as u128)
        .with_unrecorded_blocks(&unrecorded_blocks)
        .build();

    let new_cost_per_byte = 100;
    let (recorded_heights, recorded_cost) = unrecorded_blocks.iter().fold(
        (vec![], 0),
        |(mut range, cost), (height, bytes)| {
            range.push(height);
            (range, cost + bytes * new_cost_per_byte)
        },
    );
    let min = *recorded_heights.iter().min().unwrap();
    let max = *recorded_heights.iter().max().unwrap();
    let recorded_range: Vec<u32> = (*min..(max + 1)).collect();
    let recorded_bytes = 500;

    let old_da_gas_price = updater.new_scaled_da_gas_price;

    // when
    updater
        .update_da_record_data(
            &recorded_range,
            recorded_bytes,
            recorded_cost as u128,
            &mut unrecorded_blocks,
        )
        .unwrap();

    // then
    let new_da_gas_price = updater.new_scaled_da_gas_price;
    assert_eq!(old_da_gas_price, new_da_gas_price);
}
