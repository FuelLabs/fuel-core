use crate::v1::{
    tests::UpdaterBuilder,
    BlockBytes,
    Error,
    RecordedBlock,
};
use proptest::{
    prelude::Rng,
    prop_compose,
    proptest,
};
use rand::SeedableRng;

#[test]
fn update_da_record_data__increases_block() {
    // given
    let da_recorded_block_height = 0;
    let mut updater = UpdaterBuilder::new()
        .with_da_recorded_block_height(da_recorded_block_height)
        .build();

    let blocks = vec![
        RecordedBlock {
            height: 1,
            block_bytes: 1000,
            block_cost: 100,
        },
        RecordedBlock {
            height: 2,
            block_bytes: 1000,
            block_cost: 100,
        },
    ];

    // when
    updater.update_da_record_data(&blocks).unwrap();

    // then
    let expected = 2;
    let actual = updater.da_recorded_block_height;
    assert_eq!(actual, expected);
}

#[test]
fn update_da_record_data__throws_error_if_out_of_order() {
    // given
    let da_recorded_block_height = 0;
    let mut updater = UpdaterBuilder::new()
        .with_da_recorded_block_height(da_recorded_block_height)
        .build();

    let blocks = vec![
        RecordedBlock {
            height: 1,
            block_bytes: 1000,
            block_cost: 100,
        },
        RecordedBlock {
            height: 3,
            block_bytes: 1000,
            block_cost: 100,
        },
    ];

    // when
    let actual_error = updater.update_da_record_data(&blocks).unwrap_err();

    // then
    let expected_error = Error::SkippedDABlock {
        expected: 2,
        got: 3,
    };
    assert_eq!(actual_error, expected_error);
}

#[test]
fn update_da_record_data__updates_cost_per_byte() {
    // given
    let da_cost_per_byte = 20;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .build();

    let block_bytes = 1000;
    let new_cost_per_byte = 100;
    let block_cost = block_bytes * new_cost_per_byte;
    let blocks = vec![RecordedBlock {
        height: 1,
        block_bytes,
        block_cost,
    }];
    // when
    updater.update_da_record_data(&blocks).unwrap();

    // then
    let expected = new_cost_per_byte as u128;
    let actual = updater.latest_da_cost_per_byte;
    assert_eq!(actual, expected);
}

#[test]
fn update_da_record_data__updates_known_total_cost() {
    // given
    let da_cost_per_byte = 20;
    let da_recorded_block_height = 10;
    let l2_block_height = 15;
    let projected_total_cost = 2000;
    let known_total_cost = 1500;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_da_recorded_block_height(da_recorded_block_height)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost)
        .with_known_total_cost(known_total_cost)
        .build();

    let block_bytes = 1000;
    let block_cost = 100;
    let blocks = vec![
        RecordedBlock {
            height: 11,
            block_bytes,
            block_cost,
        },
        RecordedBlock {
            height: 12,
            block_bytes,
            block_cost,
        },
        RecordedBlock {
            height: 13,
            block_bytes,
            block_cost,
        },
    ];
    // when
    updater.update_da_record_data(&blocks).unwrap();

    // then
    let actual = updater.latest_known_total_da_cost_excess;
    let expected = known_total_cost + (3 * block_cost as u128);
    assert_eq!(actual, expected);
}

#[test]
fn update_da_record_data__if_da_height_matches_l2_height_prjected_and_known_match() {
    // given
    let da_cost_per_byte = 20;
    let da_recorded_block_height = 10;
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
        .with_da_recorded_block_height(da_recorded_block_height)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost as u128)
        .with_known_total_cost(known_total_cost as u128)
        .with_unrecorded_blocks(unrecorded_blocks)
        .build();

    let block_bytes = 1000;
    let new_cost_per_byte = 100;
    let block_cost = block_bytes * new_cost_per_byte;
    let blocks = vec![
        RecordedBlock {
            height: 11,
            block_bytes,
            block_cost,
        },
        RecordedBlock {
            height: 12,
            block_bytes,
            block_cost,
        },
        RecordedBlock {
            height: 13,
            block_bytes,
            block_cost,
        },
    ];
    // when
    updater.update_da_record_data(&blocks).unwrap();

    // then
    assert_eq!(updater.l2_block_height, updater.da_recorded_block_height);
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
    let da_recorded_block_height = 10;
    let l2_block_height = 15;
    let known_total_cost = 1500;
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
            block_bytes: 1200,
        },
        BlockBytes {
            height: 15,
            block_bytes: 3000,
        },
    ];
    unrecorded_blocks.extend(remaining.clone());
    let guessed_cost: u64 = unrecorded_blocks
        .iter()
        .map(|block| block.block_bytes * da_cost_per_byte)
        .sum();
    let projected_total_cost = known_total_cost + guessed_cost;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte as u128)
        .with_da_recorded_block_height(da_recorded_block_height)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost as u128)
        .with_known_total_cost(known_total_cost as u128)
        .with_unrecorded_blocks(unrecorded_blocks)
        .build();

    let block_bytes = 1000;
    let new_cost_per_byte = 100;
    let block_cost = block_bytes * new_cost_per_byte;
    let blocks = vec![
        RecordedBlock {
            height: 11,
            block_bytes,
            block_cost,
        },
        RecordedBlock {
            height: 12,
            block_bytes,
            block_cost,
        },
        RecordedBlock {
            height: 13,
            block_bytes,
            block_cost,
        },
    ];
    // when
    updater.update_da_record_data(&blocks).unwrap();

    // then
    let actual = updater.projected_total_da_cost;
    let new_known_total_cost = known_total_cost + 3 * block_cost;
    let guessed_part: u64 = remaining
        .iter()
        .map(|block| block.block_bytes * new_cost_per_byte)
        .sum();
    let expected = new_known_total_cost + guessed_part;
    assert_eq!(actual, expected as u128);
}

prop_compose! {
    fn arb_vec_of_da_blocks()(last_da_block: u32, count in 1..123usize, rng_seed: u64) -> Vec<RecordedBlock> {
        let rng = &mut rand::rngs::StdRng::seed_from_u64(rng_seed);
        let mut blocks = Vec::with_capacity(count);
        for i in 0..count {
            let block_bytes = rng.gen_range(100..131_072);
            let cost_per_byte = rng.gen_range(1..1000000);
            let block_cost = block_bytes * cost_per_byte;
            blocks.push(RecordedBlock {
                height: last_da_block + 1 + i as u32,
                block_bytes,
                block_cost,
            });
        }
        blocks
    }
}

prop_compose! {
    fn reward_greater_than_cost_with_da_blocks()(cost: u64, extra: u64, blocks in arb_vec_of_da_blocks()) -> (u128, u128, Vec<RecordedBlock>) {
        let cost_from_blocks = blocks.iter().map(|block| block.block_cost as u128).sum::<u128>();
        let reward = cost as u128 + cost_from_blocks + extra as u128;
        (cost as u128, reward, blocks)
    }
}

proptest! {
    #[test]
    fn update_da_record_data__when_reward_is_greater_than_cost_will_zero_cost_and_subtract_from_reward(
        (cost, reward, blocks) in reward_greater_than_cost_with_da_blocks()
    ) {
        _update_da_record_data__when_reward_is_greater_than_cost_will_zero_cost_and_subtract_from_reward(
            cost,
            reward,
            blocks
        )
    }
}

fn _update_da_record_data__when_reward_is_greater_than_cost_will_zero_cost_and_subtract_from_reward(
    known_total_cost: u128,
    total_rewards: u128,
    blocks: Vec<RecordedBlock>,
) {
    // given
    let da_cost_per_byte = 20;
    let da_recorded_block_height = blocks.first().unwrap().height - 1;
    let l2_block_height = 15;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_da_recorded_block_height(da_recorded_block_height)
        .with_l2_block_height(l2_block_height)
        .with_known_total_cost(known_total_cost)
        .with_total_rewards(total_rewards)
        .build();

    let new_costs = blocks.iter().map(|block| block.block_cost).sum::<u64>();

    // when
    updater.update_da_record_data(&blocks).unwrap();

    // then
    let expected = total_rewards - new_costs as u128 - known_total_cost;
    let actual = updater.total_da_rewards_excess;
    assert_eq!(actual, expected);

    let expected = 0;
    let actual = updater.latest_known_total_da_cost_excess;
    assert_eq!(actual, expected);
}

prop_compose! {
    fn cost_greater_than_reward_with_da_blocks()(reward: u64, extra: u64, blocks in arb_vec_of_da_blocks()) -> (u128, u128, Vec<RecordedBlock>) {
        let cost_from_blocks = blocks.iter().map(|block| block.block_cost as u128).sum::<u128>();
        let cost = reward as u128 + cost_from_blocks + extra as u128;
        (cost, reward as u128, blocks)
    }
}

proptest! {
    #[test]
    fn update_da_record_data__when_cost_is_greater_than_reward_will_zero_reward_and_subtract_from_cost(
        (cost, reward, blocks) in cost_greater_than_reward_with_da_blocks()
    ) {
        _update_da_record_data__when_cost_is_greater_than_reward_will_zero_reward_and_subtract_from_cost(
            cost,
            reward,
            blocks
        )
    }
}

fn _update_da_record_data__when_cost_is_greater_than_reward_will_zero_reward_and_subtract_from_cost(
    known_total_cost: u128,
    total_rewards: u128,
    blocks: Vec<RecordedBlock>,
) {
    // given
    let da_cost_per_byte = 20;
    let da_recorded_block_height = blocks.first().unwrap().height - 1;
    let l2_block_height = 15;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_da_recorded_block_height(da_recorded_block_height)
        .with_l2_block_height(l2_block_height)
        .with_known_total_cost(known_total_cost)
        .with_total_rewards(total_rewards)
        .build();

    let new_costs = blocks.iter().map(|block| block.block_cost).sum::<u64>();

    // when
    updater.update_da_record_data(&blocks).unwrap();

    // then
    let expected = 0;
    let actual = updater.total_da_rewards_excess;
    assert_eq!(actual, expected);

    let expected = known_total_cost + new_costs as u128 - total_rewards;
    let actual = updater.latest_known_total_da_cost_excess;
    assert_eq!(actual, expected);
}
