#[allow(non_snake_case)]
use super::*;

struct UpdaterBuilder {
    starting_block: u32,
    da_recorded_block_height: u32,
    da_cost_per_byte: u64,
}

impl UpdaterBuilder {
    fn new() -> Self {
        Self {
            starting_block: 0,
            da_recorded_block_height: 0,
            da_cost_per_byte: 0,
        }
    }

    fn with_starting_block(mut self, starting_block: u32) -> Self {
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

    fn build(self) -> AlgorithmUpdaterV1 {
        AlgorithmUpdaterV1::new(self.starting_block, self.da_recorded_block_height, self.da_cost_per_byte)
    }
}

#[test]
fn update__l2_block() {
    // given
    let starting_block = 0;

    let mut updater = UpdaterBuilder::new()
        .with_starting_block(starting_block)
        .build();

    let update = UpdateValues::L2Block {
        height: 1,
        block_reward: 100,
        block_bytes: 1000,
    };

    // when
    updater.update(update).unwrap();

    //  then
    let expected = starting_block + 1;
    let actual = updater.l2_block_height;
    assert_eq!(actual, expected);
}

#[test]
fn update__skipped_block_height_throws_error() {
    // given
    let starting_block = 0;
    let mut updater = UpdaterBuilder::new()
        .with_starting_block(starting_block)
        .build();

    let update = UpdateValues::L2Block {
        height: 2,
        block_reward: 100,
        block_bytes: 1000,
    };

    // when
    let actual_error = updater.update(update).unwrap_err();

    // then
    let expected_error = Error::SkippedL2Block {
        expected: starting_block + 1,
        got: 2,
    };
    assert_eq!(actual_error, expected_error);
}

#[test]
fn update__da_recorded_block() {
    // given
    let da_recorded_block_height = 0;
    let mut updater = UpdaterBuilder::new()
        .with_da_recorded_block_height(da_recorded_block_height)
        .build();

    let update = UpdateValues::DARecording {
        blocks: vec![RecordedBlock {
            height: 1,
            block_bytes: 1000,
            block_cost: 100,
        }, RecordedBlock {
            height: 2,
            block_bytes: 1000,
            block_cost: 100,
        }
        ],
    };

    // when
    updater.update(update).unwrap();

    // then
    let expected = 2;
    let actual = updater.da_recorded_block_height;
    assert_eq!(actual, expected);
}

#[test]
fn update__da_recorded_blocks_must_come_in_order() {
    // given
    let da_recorded_block_height = 0;
    let mut updater = UpdaterBuilder::new()
        .with_da_recorded_block_height(da_recorded_block_height)
        .build();

    let update = UpdateValues::DARecording {
        blocks: vec![RecordedBlock {
            height: 1,
            block_bytes: 1000,
            block_cost: 100,
        }, RecordedBlock {
            height: 3,
            block_bytes: 1000,
            block_cost: 100,
        }
        ],
    };

    // when
    let actual_error = updater.update(update).unwrap_err();

    // then
    let expected_error = Error::SkippedDABlock {
        expected: 2,
        got: 3,
    };
    assert_eq!(actual_error, expected_error);
}

#[test]
fn update__l2_block_updates_projected_cost() {
    // given
    let da_cost_per_byte = 20;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .build();

    let block_bytes = 1000;
    let update = UpdateValues::L2Block {
        height: 1,
        block_reward: 100,
        block_bytes,
    };

    // when
    updater.update(update).unwrap();

    // then
    let expected = block_bytes * da_cost_per_byte;
    let actual = updater.projected_total_cost;
    assert_eq!(actual, expected);
}

#[test]
fn update__da_recorded_blocks_updates_cost_per_byte() {
    // given
    let da_cost_per_byte = 20;
    let mut updater = UpdaterBuilder::new()
        .with_da_cost_per_byte(da_cost_per_byte)
        .build();

    let block_bytes = 1000;
    let new_cost_per_byte = 100;
    let block_cost = block_bytes * new_cost_per_byte;
    let update = UpdateValues::DARecording {
        blocks: vec![RecordedBlock {
            height: 1,
            block_bytes,
            block_cost,
        }
        ],
    };

    // when
    updater.update(update).unwrap();

    // then
    let expected = new_cost_per_byte;
    let actual = updater.latest_da_cost_per_byte;
    assert_eq!(actual, expected);
}