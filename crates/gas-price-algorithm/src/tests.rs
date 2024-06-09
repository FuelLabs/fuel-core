#![allow(non_snake_case)]
use super::*;

struct UpdaterBuilder {
    starting_block: u32,
    da_recorded_block_height: u32,
    da_cost_per_byte: u64,
    project_total_cost: u64,
    latest_known_total_cost: u64,
    unrecorded_blocks: Vec<BlockBytes>,
}

impl UpdaterBuilder {
    fn new() -> Self {
        Self {
            starting_block: 0,
            da_recorded_block_height: 0,
            da_cost_per_byte: 0,
            project_total_cost: 0,
            latest_known_total_cost: 0,
            unrecorded_blocks: vec![],
        }
    }

    fn with_l2_block_height(mut self, starting_block: u32) -> Self {
        self.starting_block = starting_block;
        self
    }

    fn with_da_recorded_block_height(mut self, da_recorded_block_height: u32) -> Self {
        self.da_recorded_block_height = da_recorded_block_height;
        self
    }

    fn with_da_cost_per_byte(mut self, da_cost_per_byte: u64) -> Self {
        self.da_cost_per_byte = da_cost_per_byte;
        self
    }

    fn with_projected_total_cost(mut self, projected_total_cost: u64) -> Self {
        self.project_total_cost = projected_total_cost;
        self
    }

    fn with_known_total_cost(mut self, latest_known_total_cost: u64) -> Self {
        self.latest_known_total_cost = latest_known_total_cost;
        self
    }

    fn with_unrecorded_blocks(mut self, unrecorded_blocks: Vec<BlockBytes>) -> Self {
        self.unrecorded_blocks = unrecorded_blocks;
        self
    }


    fn build(self) -> AlgorithmUpdaterV1 {
        AlgorithmUpdaterV1 {
            l2_block_height: self.starting_block,
            da_recorded_block_height: self.da_recorded_block_height,
            latest_da_cost_per_byte: self.da_cost_per_byte,
            projected_total_cost: self.project_total_cost,
            latest_known_total_cost: self.latest_known_total_cost,
            unrecorded_blocks: self.unrecorded_blocks,
        }
    }
}

#[test]
fn l2_block_update__updates_l2_block() {
    // given
    let starting_block = 0;

    let mut updater = UpdaterBuilder::new()
        .with_l2_block_height(starting_block)
        .build();

    let height = 1;
    let block_reward = 100;
let block_bytes = 1000;


    // when
    updater.l2_block_update(height, block_reward, block_bytes).unwrap();

    //  then
    let expected = starting_block + 1;
    let actual = updater.l2_block_height;
    assert_eq!(actual, expected);
}

#[test]
fn l2_block_update__skipped_block_height_throws_error() {
    // given
    let starting_block = 0;
    let mut updater = UpdaterBuilder::new()
        .with_l2_block_height(starting_block)
        .build();

    let height = 2;
    let block_reward = 100;
    let block_bytes = 1000;

    // when
    let actual_error = updater.l2_block_update(height, block_reward, block_bytes).unwrap_err();

    // then
    let expected_error = Error::SkippedL2Block {
        expected: starting_block + 1,
        got: 2,
    };
    assert_eq!(actual_error, expected_error);
}

#[test]
fn update_da_record_data__increases_block() {
    // given
    let da_recorded_block_height = 0;
    let mut updater = UpdaterBuilder::new()
        .with_da_recorded_block_height(da_recorded_block_height)
        .build();

    let blocks =
         vec![RecordedBlock {
            height: 1,
            block_bytes: 1000,
            block_cost: 100,
        }, RecordedBlock {
            height: 2,
            block_bytes: 1000,
            block_cost: 100,
        }];

    // when
    updater.update_da_record_data(blocks).unwrap();

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

    let blocks = vec![RecordedBlock {
        height: 1,
        block_bytes: 1000,
        block_cost: 100,
    }, RecordedBlock {
        height: 3,
        block_bytes: 1000,
        block_cost: 100,
    }
    ];

    // when
    let actual_error = updater.update_da_record_data(blocks).unwrap_err();

    // then
    let expected_error = Error::SkippedDABlock {
        expected: 2,
        got: 3,
    };
    assert_eq!(actual_error, expected_error);
}

#[test]
fn l2_block_update__updates_projected_cost() {
    // given
    let da_cost_per_byte = 20;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .build();

    let height = 1;
    let block_reward = 100;
    let block_bytes = 1000;

    // when
    updater.l2_block_update(height, block_reward, block_bytes).unwrap();

    // then
    let expected = block_bytes * da_cost_per_byte;
    let actual = updater.projected_total_cost;
    assert_eq!(actual, expected);
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
    let blocks =
         vec![RecordedBlock {
            height: 1,
            block_bytes,
            block_cost,
        }
        ];
    // when
    updater.update_da_record_data(blocks).unwrap();

    // then
    let expected = new_cost_per_byte;
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
        let blocks =  vec![RecordedBlock {
            height: 11,
            block_bytes,
            block_cost,
        }, RecordedBlock {
            height: 12,
            block_bytes,
            block_cost,
        }, RecordedBlock {
            height: 13,
            block_bytes,
            block_cost,
        }
        ];
    // when
    updater.update_da_record_data(blocks).unwrap();

    // then
    let actual = updater.latest_known_total_cost;
    let expected = known_total_cost + 3 * block_cost;
    assert_eq!(actual, expected);
}

#[test]
fn update__da_block_updates_projected_total_cost_with_known_and_guesses_on_top() {
    // given
    let da_cost_per_byte = 20;
    let da_recorded_block_height = 10;
    let l2_block_height = 15;
    let known_total_cost = 1500;
    let mut unrecorded_blocks = vec![BlockBytes {
        height: 11,
        block_bytes: 1000,
    }, BlockBytes {
        height: 12,
        block_bytes: 2000,
    }, BlockBytes {
        height: 13,
        block_bytes: 1500,
    }];

    let remaining = vec![BlockBytes {
        height: 14,
        block_bytes: 1200,
    }, BlockBytes {
        height: 15,
        block_bytes: 3000,
    }];
    unrecorded_blocks.extend(remaining.clone());
    let guessed_cost: u64 = unrecorded_blocks.iter().map(|block| block.block_bytes * da_cost_per_byte).sum();
    let projected_total_cost = known_total_cost + guessed_cost;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .with_da_recorded_block_height(da_recorded_block_height)
        .with_l2_block_height(l2_block_height)
        .with_projected_total_cost(projected_total_cost)
        .with_known_total_cost(known_total_cost)
        .with_unrecorded_blocks(unrecorded_blocks)
        .build();

    dbg!(updater.projected_total_cost);

    let block_bytes = 1000;
    let new_cost_per_byte = 100;
    let block_cost = block_bytes * new_cost_per_byte;
    let blocks =
         vec![RecordedBlock {
            height: 11,
            block_bytes,
            block_cost,
        }, RecordedBlock {
            height: 12,
            block_bytes,
            block_cost,
        }, RecordedBlock {
            height: 13,
            block_bytes,
            block_cost,
        }
        ];
    // when
    updater.update_da_record_data(blocks).unwrap();

    // then
    let actual = updater.projected_total_cost;
    let new_known_total_cost = known_total_cost + 3 * block_cost;
    let guessed_part: u64 = remaining.iter().map(|block| block.block_bytes * new_cost_per_byte).sum();
    let expected = new_known_total_cost + guessed_part;
    assert_eq!(actual, expected);
}