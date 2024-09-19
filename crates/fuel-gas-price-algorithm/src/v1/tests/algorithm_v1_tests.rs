use crate::v1::{
    tests::UpdaterBuilder,
    AlgorithmV1,
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
fn worst_case__same_block_gives_new_exec_price() {
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
    let expected = new_exec_price + new_da_gas_price;
    assert_eq!(expected, actual);
}
