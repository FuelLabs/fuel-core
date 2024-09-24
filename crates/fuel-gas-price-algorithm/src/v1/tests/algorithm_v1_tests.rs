use std::num::NonZeroU64;

use crate::v1::{
    tests::UpdaterBuilder,
    AlgorithmUpdaterV1,
    AlgorithmV1,
    ClampedPercentage,
};
use proptest::prelude::*;

#[test]
fn calculate__returns_sum_of_da_and_exec_gas_prices() {
    // given
    let starting_exec_gas_price = 100;
    let starting_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_gas_price_denominator = 1;
    let block_bytes = 500u64;
    let starting_reward = starting_cost + block_bytes * latest_gas_per_byte;
    let updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(starting_da_gas_price)
        .with_da_p_component(da_gas_price_denominator)
        .with_total_rewards(starting_reward as u128)
        .with_known_total_cost(starting_cost as u128)
        .with_projected_total_cost(starting_cost as u128)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate();

    // then
    let expected = starting_exec_gas_price + starting_da_gas_price;
    assert_eq!(expected, actual);
}

fn _worst_case__correctly_calculates_value(
    new_exec_price: u64,
    new_da_gas_price: u64,
    for_height: u32,
    block_horizon: u32,
    exec_price_percentage: u64,
    da_gas_price_percentage: u64,
) {
    // given
    let algorithm = AlgorithmV1 {
        new_exec_price,
        exec_price_percentage,
        new_da_gas_price,
        da_gas_price_percentage,
        for_height,
    };

    // when
    let target_height = for_height.saturating_add(block_horizon);
    let actual = algorithm.worst_case(target_height);

    // then
    let mut expected_exec_price = new_exec_price;
    let mut expected_da_gas_price = new_da_gas_price;

    for _ in 0..block_horizon {
        let change_amount = expected_exec_price
            .saturating_mul(exec_price_percentage)
            .saturating_div(100);
        expected_exec_price = expected_exec_price.saturating_add(change_amount);

        let change_amount = expected_da_gas_price
            .saturating_mul(da_gas_price_percentage)
            .saturating_div(100);
        expected_da_gas_price = expected_da_gas_price.saturating_add(change_amount);
    }

    let expected = expected_exec_price.saturating_add(expected_da_gas_price);

    dbg!(actual, expected);
    assert!(actual >= expected);
}

proptest! {
    #[test]
    fn worst_case__correctly_calculates_value(
        exec_price: u64,
        da_price: u64,
        starting_height: u32,
        block_horizon in 0..10_000u32,
        exec_percentage: u64,
        da_percentage: u64,
    ) {
        _worst_case__correctly_calculates_value(exec_price, da_price, starting_height, block_horizon, exec_percentage, da_percentage);
    }
}

proptest! {
    #[test]
    fn worst_case__never_overflows(
        exec_price: u64,
        da_price: u64,
        starting_height: u32,
        block_horizon in 0..10_000u32,
        exec_percentage: u64,
        da_percentage: u64,
    ) {
        // given
        let algorithm = AlgorithmV1 {
            new_exec_price: exec_price,
            exec_price_percentage: exec_percentage,
            new_da_gas_price: da_price,
            da_gas_price_percentage: da_percentage,
            for_height: starting_height,
        };

        // when
        let target_height = starting_height.saturating_add(block_horizon);
        let _ = algorithm.worst_case(target_height);

        // then
        // doesn't panic with an overflow
    }
}

#[test]
fn worst_case__same_block_gives_the_same_value_as_calculate() {
    // given
    let new_exec_price = 1000;
    let exec_price_percentage = 10;
    let new_da_gas_price = 1000;
    let da_gas_price_percentage = 10;
    let for_height = 10;
    let algorithm = AlgorithmV1 {
        new_exec_price,
        exec_price_percentage,
        new_da_gas_price,
        da_gas_price_percentage,
        for_height,
    };

    // when
    let delta = 0;
    let target_height = for_height + delta;
    let actual = algorithm.worst_case(target_height);

    // then
    let expected = algorithm.calculate();
    assert_eq!(expected, actual);
}

#[test]
fn da_change_should_not_panic() {
    const HEIGHT: u32 = 0;

    // The following values are chosen to make the `p.saturating_add(d)` result in
    // i128::MIN, which would cause a panic if the calculation is not using `saturating_abs()`.
    const LAST_PROFIT: i128 = i128::MIN / 2;
    const TOTAL_DA_REWARDS_EXCESS: i128 = i128::MAX / 2;
    const P: i64 = 1;
    const D: i64 = 1;

    let mut updater = AlgorithmUpdaterV1 {
        new_scaled_exec_price: 0,
        min_exec_gas_price: 0,
        exec_gas_price_change_percent: 0,
        l2_block_height: HEIGHT,
        l2_block_fullness_threshold_percent: ClampedPercentage::new(0),
        new_scaled_da_gas_price: 0,
        gas_price_factor: NonZeroU64::new(1).unwrap(),
        min_da_gas_price: 0,
        max_da_gas_price_change_percent: 0,
        total_da_rewards_excess: TOTAL_DA_REWARDS_EXCESS as u128,
        da_recorded_block_height: 0,
        latest_known_total_da_cost_excess: 0,
        projected_total_da_cost: 0,
        da_p_component: P,
        da_d_component: D,
        last_profit: LAST_PROFIT,
        second_to_last_profit: 0,
        latest_da_cost_per_byte: 0,
        unrecorded_blocks: vec![],
    };

    let _ =
        updater.update_l2_block_data(HEIGHT + 1, 0, NonZeroU64::new(1).unwrap(), 0, 0);
}
