use super::*;
use fuel_gas_price_algorithm::v1::AlgorithmUpdaterV1;
use std::{
    collections::BTreeMap,
    num::NonZeroU64,
    ops::Range,
};

pub mod da_cost_per_byte;

pub struct SimulationResults {
    pub gas_prices: Vec<u64>,
    pub exec_gas_prices: Vec<u64>,
    pub da_gas_prices: Vec<u64>,
    pub fullness: Vec<(u64, u64)>,
    pub bytes_and_costs: Vec<(u64, u64)>,
    pub actual_profit: Vec<i128>,
    pub projected_profit: Vec<i128>,
    pub pessimistic_costs: Vec<u128>,
}

#[derive(Clone, Debug)]
pub struct Simulator {
    da_cost_per_byte: Vec<u64>,
}

// (usize, ((u64, u64), &'a Option<(Range<u32>, u128)
struct BlockData {
    fullness: u64,
    bytes: u64,
    maybe_da_block: Option<(Range<u32>, u128)>,
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
        da_finalization_rate: usize,
    ) -> SimulationResults {
        let capacity = 30_000_000;
        let gas_per_byte = 63;
        let max_block_bytes = capacity / gas_per_byte;
        let size = self.da_cost_per_byte.len();
        let fullness_and_bytes = fullness_and_bytes_per_block(size, capacity);

        let l2_blocks = fullness_and_bytes.clone().into_iter();
        let da_blocks = self.calculate_da_blocks(
            update_period,
            da_finalization_rate,
            &fullness_and_bytes,
        );

        let blocks = l2_blocks
            .zip(da_blocks.iter())
            .map(|((fullness, bytes), maybe_da_block)| BlockData {
                fullness,
                bytes,
                maybe_da_block: maybe_da_block.clone(),
            })
            .enumerate();

        let updater = self.build_updater(da_p_component, da_d_component);

        self.execute_simulation(
            capacity,
            max_block_bytes,
            fullness_and_bytes,
            blocks,
            updater,
        )
    }

    fn build_updater(
        &self,
        da_p_component: i64,
        da_d_component: i64,
    ) -> AlgorithmUpdaterV1 {
        // Scales the gas price internally, value is arbitrary
        let gas_price_factor = 100;
        let updater = AlgorithmUpdaterV1 {
            min_exec_gas_price: 10,
            min_da_gas_price: 10,
            // Change to adjust where the exec gas price starts on block 0
            new_scaled_exec_price: 10 * gas_price_factor,
            // Change to adjust where the da gas price starts on block 0
            new_scaled_da_gas_price: 100,
            gas_price_factor: NonZeroU64::new(gas_price_factor).unwrap(),
            l2_block_height: 0,
            // Choose the ideal fullness percentage for the L2 block
            l2_block_fullness_threshold_percent: 50u8.into(),
            // Increase to make the exec price change faster
            exec_gas_price_change_percent: 2,
            // Increase to make the da price change faster
            max_da_gas_price_change_percent: 10,
            total_da_rewards_excess: 0,
            da_recorded_block_height: 0,
            // Change to adjust the cost per byte of the DA on block 0
            latest_da_cost_per_byte: 0,
            projected_total_da_cost: 0,
            latest_known_total_da_cost_excess: 0,
            unrecorded_blocks: BTreeMap::new(),
            da_p_component,
            da_d_component,
            last_profit: 0,
            second_to_last_profit: 0,
        };
        updater
    }

    fn execute_simulation<'a>(
        &self,
        capacity: u64,
        max_block_bytes: u64,
        fullness_and_bytes: Vec<(u64, u64)>,
        blocks: impl Iterator<Item = (usize, BlockData)>,
        mut updater: AlgorithmUpdaterV1,
    ) -> SimulationResults {
        let mut gas_prices = vec![];
        let mut exec_gas_prices = vec![];
        let mut da_gas_prices = vec![];
        let mut actual_reward_totals = vec![];
        let mut projected_cost_totals = vec![];
        let mut actual_costs = vec![];
        let mut pessimistic_costs = vec![];
        for (index, block_data) in blocks {
            let BlockData {
                fullness,
                bytes,
                maybe_da_block: da_block,
            } = block_data;
            let height = index as u32 + 1;
            exec_gas_prices.push(updater.new_scaled_exec_price);
            da_gas_prices.push(updater.new_scaled_da_gas_price);
            let gas_price = updater.algorithm().calculate();
            gas_prices.push(gas_price);
            let total_fee = gas_price as u128 * fullness as u128;
            updater
                .update_l2_block_data(
                    height,
                    fullness,
                    capacity.try_into().unwrap(),
                    bytes,
                    total_fee,
                )
                .unwrap();
            pessimistic_costs
                .push(max_block_bytes as u128 * updater.latest_da_cost_per_byte);
            actual_reward_totals.push(updater.total_da_rewards_excess);
            projected_cost_totals.push(updater.projected_total_da_cost);

            // Update DA blocks on the occasion there is one
            if let Some((range, cost)) = da_block {
                for height in range.to_owned() {
                    updater
                        .update_da_record_data(height..(height + 1), cost)
                        .unwrap();
                    actual_costs.push(updater.latest_known_total_da_cost_excess)
                }
            }
        }
        let (fullness_without_capacity, bytes): (Vec<_>, Vec<_>) =
            fullness_and_bytes.iter().cloned().unzip();
        let fullness: Vec<_> = fullness_without_capacity
            .iter()
            .map(|&fullness| (fullness, capacity))
            .collect();
        let bytes_and_costs: Vec<_> = bytes
            .iter()
            .zip(self.da_cost_per_byte.iter())
            .map(|(bytes, cost_per_byte)| (*bytes, (*bytes * cost_per_byte) as u64))
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

        SimulationResults {
            gas_prices,
            exec_gas_prices,
            da_gas_prices,
            fullness,
            bytes_and_costs,
            actual_profit,
            projected_profit,
            pessimistic_costs,
        }
    }

    fn calculate_da_blocks(
        &self,
        da_recording_rate: usize,
        da_finalization_rate: usize,
        fullness_and_bytes: &Vec<(u64, u64)>,
    ) -> Vec<Option<(Range<u32>, u128)>> {
        let l2_blocks_with_no_da_blocks =
            std::iter::repeat(None).take(da_finalization_rate);
        let (_, da_blocks) = fullness_and_bytes
            .iter()
            .zip(self.da_cost_per_byte.iter())
            .enumerate()
            .fold(
                (vec![], vec![]),
                |(mut delayed, mut recorded),
                 (index, ((_fullness, bytes), cost_per_byte))| {
                    let total_cost = *bytes * cost_per_byte;

                    let height = index as u32 + 1;
                    let converted = (height, bytes, total_cost);
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
                let min = heights_iter.clone().min().unwrap();
                let max = heights_iter.max().unwrap();
                let cost: u128 = list.iter().map(|(_, _, cost)| *cost as u128).sum();
                (min..(max + 1), cost)
            })
        });
        l2_blocks_with_no_da_blocks.chain(da_block_ranges).collect()
    }
}

// Naive Fourier series
fn gen_noisy_signal(input: f64, components: &[f64]) -> f64 {
    components
        .iter()
        .fold(0f64, |acc, &c| acc + f64::sin(input / c))
        / components.len() as f64
}

fn noisy_fullness<T: TryInto<f64>>(input: T) -> f64
where
    <T as TryInto<f64>>::Error: core::fmt::Debug,
{
    const COMPONENTS: &[f64] = &[-30.0, 40.0, 700.0, -340.0, 400.0];
    let input = input.try_into().unwrap();
    gen_noisy_signal(input, COMPONENTS)
}

fn fullness_and_bytes_per_block(size: usize, capacity: u64) -> Vec<(u64, u64)> {
    let mut rng = StdRng::seed_from_u64(888);

    let fullness_noise: Vec<_> = std::iter::repeat(())
        .take(size)
        .map(|_| rng.gen_range(-0.25..0.25))
        .map(|val| val * capacity as f64)
        .collect();

    const ROUGH_GAS_TO_BYTE_RATIO: f64 = 0.01;
    let bytes_scale: Vec<_> = std::iter::repeat(())
        .take(size)
        .map(|_| rng.gen_range(0.5..1.0))
        .map(|x| x * ROUGH_GAS_TO_BYTE_RATIO)
        .collect();

    (0usize..size)
        .map(|val| val as f64)
        .map(noisy_fullness)
        .map(|signal| (0.5 * signal + 0.5) * capacity as f64) // Scale and shift so it's between 0 and capacity
        .zip(fullness_noise)
        .map(|(fullness, noise)| fullness + noise)
        .map(|x| f64::min(x, capacity as f64))
        .map(|x| f64::max(x, 5.0))
        .zip(bytes_scale)
        .map(|(fullness, bytes_scale)| {
            let bytes = fullness * bytes_scale;
            (fullness, bytes)
        })
        .map(|(fullness, bytes)| (fullness as u64, std::cmp::max(bytes as u64, 1)))
        .collect()
}
