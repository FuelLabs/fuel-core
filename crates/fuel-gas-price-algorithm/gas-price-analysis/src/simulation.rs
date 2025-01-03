use std::{
    collections::BTreeMap,
    num::NonZeroU64,
};

use fuel_gas_price_algorithm::v1::{
    AlgorithmUpdaterV1,
    L2ActivityTracker,
};

use super::*;

pub mod da_cost_per_byte;

pub(crate) type Fullness = u64;
pub(crate) type Capacity = u64;

pub struct SimulationResults {
    pub gas_prices: Vec<u64>,
    pub exec_gas_prices: Vec<u64>,
    pub da_gas_prices: Vec<u64>,
    pub fullness: Vec<(Fullness, Capacity)>,
    pub bytes_and_costs: Vec<(u32, u64)>,
    pub actual_profit: Vec<i128>,
    pub projected_profit: Vec<i128>,
    pub pessimistic_costs: Vec<u128>,
    pub first_height: u32,
}

#[derive(Clone, Debug)]
pub struct Simulator {
    da_cost_per_byte: Vec<u64>,
}

// (usize, ((u64, u64), &'a Option<(Range<u32>, u128)
struct BlockData {
    height: u64,
    fullness: u64,
    gas_capacity: u64,
    bytes: u32,
    bytes_capacity: u64,
    maybe_da_bundle: Option<Bundle>,
}

#[derive(Clone, Debug)]
struct Bundle {
    heights: Vec<u32>,
    bytes: u32,
    cost: u128,
}

impl Simulator {
    pub fn new(da_cost_per_byte: Vec<u64>) -> Self {
        Simulator { da_cost_per_byte }
    }

    pub fn run_simulation(
        &self,
        da_p_component: i64,
        da_d_component: i64,
        update_period: usize,
        l2_blocks: &[L2BlockData],
        da_finalization_rate: usize,
    ) -> SimulationResults {
        let da_blocks =
            self.calculate_da_blocks(update_period, da_finalization_rate, l2_blocks);

        let blocks = l2_blocks.iter().zip(da_blocks.iter()).map(
            |(
                L2BlockData {
                    fullness,
                    size,
                    height,
                    gas_capacity,
                    size_capacity,
                },
                maybe_da_block,
            )| BlockData {
                height: *height,
                fullness: *fullness,
                bytes: *size,
                maybe_da_bundle: maybe_da_block.clone(),
                gas_capacity: *gas_capacity,
                bytes_capacity: *size_capacity,
            },
        );

        let updater = self.build_updater(
            da_p_component,
            da_d_component,
            l2_blocks
                .first()
                .expect("should have at least 1 l2 block")
                .height as u32,
        );

        self.execute_simulation(l2_blocks, blocks, updater)
    }

    fn build_updater(
        &self,
        da_p_component: i64,
        da_d_component: i64,
        l2_block_height: u32,
    ) -> AlgorithmUpdaterV1 {
        // Scales the gas price internally, value is arbitrary
        let gas_price_factor = 100;
        let always_normal_activity = L2ActivityTracker::new_always_normal();

        let updater = AlgorithmUpdaterV1 {
            min_exec_gas_price: 10_000_000,
            min_da_gas_price: 100_000_000,
            // Change to adjust where the exec gas price starts on first block
            new_scaled_exec_price: 10_000_000 * gas_price_factor,
            // Change to adjust where the da gas price starts on first block
            new_scaled_da_gas_price: 10_000_000 * gas_price_factor,
            gas_price_factor: NonZeroU64::new(gas_price_factor).unwrap(),
            l2_block_height,
            // Choose the ideal fullness percentage for the L2 block
            l2_block_fullness_threshold_percent: 50u8.into(),
            // Increase to make the exec price change faster
            exec_gas_price_change_percent: 2,
            // Increase to make the da price change faster
            max_da_gas_price_change_percent: 15,
            total_da_rewards_excess: 0,
            // Change to adjust the cost per byte of the DA on first block
            latest_da_cost_per_byte: 0,
            projected_total_da_cost: 0,
            latest_known_total_da_cost_excess: 0,
            da_p_component,
            da_d_component,
            last_profit: 0,
            second_to_last_profit: 0,
            l2_activity: always_normal_activity,
            unrecorded_blocks_bytes: 0,
        };

        tracing::trace!(?updater, "Built updater");

        updater
    }

    fn execute_simulation(
        &self,
        fullness_and_bytes: &[L2BlockData],
        blocks: impl Iterator<Item = BlockData>,
        mut updater: AlgorithmUpdaterV1,
    ) -> SimulationResults {
        let mut gas_prices = vec![];
        let mut exec_gas_prices = vec![];
        let mut da_gas_prices = vec![];
        let mut actual_reward_totals = vec![];
        let mut projected_cost_totals = vec![];
        let mut actual_costs = vec![];
        let mut pessimistic_costs = vec![];
        let mut unrecorded_blocks = BTreeMap::new();
        for block_data in blocks {
            tracing::trace!(%block_data.height,
                "Processing L2 block with{} DA bundle",
                if block_data.maybe_da_bundle.is_some() {
                    ""
                } else {
                    "out"
                }
            );
            let BlockData {
                fullness,
                bytes,
                maybe_da_bundle,
                height,
                gas_capacity,
                bytes_capacity,
            } = block_data;
            let height = height as u32 + 1;
            let new_scaled_exec_price = updater.new_scaled_exec_price;
            exec_gas_prices.push(new_scaled_exec_price);
            let new_scaled_da_gas_price = updater.new_scaled_da_gas_price;
            da_gas_prices.push(new_scaled_da_gas_price);
            let gas_price = updater.algorithm().calculate();
            gas_prices.push(gas_price);
            let total_fee = gas_price as u128 * fullness as u128;
            tracing::trace!(
                "New scaled exec price: {}, New scaled da price: {}, Gas price: {}, Fullness: {}, Total fee: {}",
                new_scaled_exec_price,
                new_scaled_da_gas_price,
                gas_price,fullness,
                total_fee
            );
            updater
                .update_l2_block_data(
                    height,
                    fullness,
                    gas_capacity.try_into().unwrap(),
                    bytes as u64,
                    total_fee,
                    &mut unrecorded_blocks,
                )
                .unwrap();
            pessimistic_costs
                .push(bytes_capacity as u128 * updater.latest_da_cost_per_byte);
            actual_reward_totals.push(updater.total_da_rewards_excess);
            projected_cost_totals.push(updater.projected_total_da_cost);

            // Update DA blocks on the occasion there is one
            if let Some(bundle) = maybe_da_bundle {
                let Bundle {
                    heights,
                    bytes,
                    cost,
                } = bundle;
                for height in heights {
                    let block_heights: Vec<u32> = (height..(height) + 1).collect();
                    updater
                        .update_da_record_data(
                            &block_heights,
                            bytes,
                            cost,
                            &mut unrecorded_blocks,
                        )
                        .unwrap();
                    actual_costs.push(updater.latest_known_total_da_cost_excess)
                }
            }
        }
        let (fullness, bytes): (Vec<_>, Vec<_>) = fullness_and_bytes
            .iter()
            .map(|l2_block_data| {
                (
                    (l2_block_data.fullness, l2_block_data.gas_capacity),
                    l2_block_data.size,
                )
            })
            .unzip();
        let bytes_and_costs: Vec<_> = bytes
            .iter()
            .zip(self.da_cost_per_byte.iter())
            .map(|(bytes, da_cost_per_byte)| (*bytes, *bytes as u64 * da_cost_per_byte))
            .collect();

        let actual_profit: Vec<i128> = actual_costs
            .iter()
            .zip(actual_reward_totals.iter())
            .map(|(cost, reward)| *reward as i128 - *cost as i128)
            .collect();

        let projected_profit: Vec<i128> = projected_cost_totals
            .iter()
            .zip(actual_reward_totals.iter())
            .map(|(cost, reward)| *reward as i128 - *cost as i128)
            .collect();

        let first_height = fullness_and_bytes
            .first()
            .expect("should have at least 1 l2 block")
            .height as u32;

        SimulationResults {
            gas_prices,
            exec_gas_prices,
            da_gas_prices,
            fullness,
            bytes_and_costs,
            actual_profit,
            projected_profit,
            pessimistic_costs,
            first_height,
        }
    }

    fn calculate_da_blocks(
        &self,
        da_recording_rate: usize,
        da_finalization_rate: usize,
        fullness_and_bytes: &[L2BlockData],
    ) -> Vec<Option<Bundle>> {
        let l2_blocks_with_no_da_blocks =
            std::iter::repeat(None).take(da_finalization_rate);
        let (_, da_blocks) = fullness_and_bytes
            .iter()
            .zip(self.da_cost_per_byte.iter())
            .fold(
                (vec![], vec![]),
                |(mut delayed, mut recorded),

                 (L2BlockData { size, height, .. }, cost_per_byte)| {
                    let total_cost = *size as u64 * cost_per_byte;
                    let height = *height as u32 + 1;
                    let converted = (height, size, total_cost);
                    delayed.push(converted);
                    if delayed.len() == da_recording_rate {
                        recorded.push(Some(delayed));
                        (vec![], recorded)
                    } else {
                        recorded.push(None);
                        (delayed, recorded)
                    }
                },
            );
        let da_block_ranges = da_blocks.into_iter().map(|maybe_recorded_blocks| {
            maybe_recorded_blocks.map(|list| {
                let heights_iter = list.iter().map(|(height, _, _)| *height);
                let bytes = list.iter().map(|(_, bytes, _)| *bytes).sum();
                let cost = list.iter().map(|(_, _, cost)| *cost as u128).sum();
                Bundle {
                    heights: heights_iter.collect(),
                    bytes,
                    cost,
                }
            })
        });
        l2_blocks_with_no_da_blocks.chain(da_block_ranges).collect()
    }
}
