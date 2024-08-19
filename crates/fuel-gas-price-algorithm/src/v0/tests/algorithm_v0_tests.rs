use crate::v0::AlgorithmV0;
use proptest::prelude::*;

#[test]
fn calculate__gives_static_value() {
    // given
    let value = 100;
    let algorithm = AlgorithmV0 {
        new_exec_price: value,
        for_height: 0,
        percentage: 0,
    };

    // when
    let actual = algorithm.calculate();

    // then
    let expected = value;
    assert_eq!(expected, actual);
}

fn _worst_case__correctly_calculates_value(
    price: u64,
    starting_height: u32,
    block_horizon: u32,
    percentage: u64,
) {
    // given
    let algorithm = AlgorithmV0 {
        new_exec_price: price,
        for_height: starting_height,
        percentage,
    };

    // when
    let target_height = starting_height.saturating_add(block_horizon);
    let actual = algorithm.worst_case(target_height);

    // then
    let mut expected = price;
    for _ in 0..block_horizon {
        let change_amount = expected.saturating_mul(percentage).saturating_div(100);
        expected = expected.saturating_add(change_amount);
    }
    dbg!(actual, expected);
    assert!(actual >= expected);
}

proptest! {
    #[test]
    fn worst_case__correctly_calculates_value(
        price: u64,
        starting_height: u32,
        block_horizon in 0..10_000u32,
        percentage: u64
    ) {
        _worst_case__correctly_calculates_value(price, starting_height, block_horizon, percentage);
    }
}

proptest! {
    #[test]
    fn worst_case__never_overflows(
        price: u64,
        starting_height: u32,
        block_horizon: u32,
        percentage: u64
    ) {
        // given
        let algorithm = AlgorithmV0 {
            new_exec_price: price,
            for_height: starting_height,
            percentage,
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
    let for_height = 10;
    let percentage = 10;
    let algorithm = AlgorithmV0 {
        new_exec_price,
        for_height,
        percentage,
    };

    // when
    let delta = 0;
    let target_height = for_height + delta;
    let actual = algorithm.worst_case(target_height);

    // then
    let expected = new_exec_price;
    assert_eq!(expected, actual);
}
