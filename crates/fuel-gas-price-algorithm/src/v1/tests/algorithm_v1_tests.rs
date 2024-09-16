use crate::v1::tests::UpdaterBuilder;

#[test]
fn calculate__even_profit_maintains_price() {
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

#[test]
fn calculate__negative_profit_increase_gas_price() {
    // given
    let starting_exec_gas_price = 100;
    let starting_da_gas_price = 100;
    let starting_cost = u128::MAX;
    let latest_gas_per_byte = i32::MAX; // DA is very expensive
    let da_p_component = 100;
    let da_d_component = 10;
    let last_profit = i128::MIN;
    let last_last_profit = 0;
    let smaller_starting_reward = 0;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(starting_da_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_total_rewards(smaller_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte as u128)
        .with_last_profit(last_profit, last_last_profit)
        .build();
    let algo = updater.algorithm();
    let old_gas_price = algo.calculate();

    // when
    let height = updater.l2_block_height + 1;
    let used = 50;
    let capacity = 100u64.try_into().unwrap();
    let block_bytes = 500u64;
    let gas_price = 10;
    updater
        .update_l2_block_data(height, used, capacity, block_bytes, gas_price)
        .unwrap();

    // then
    let algo = updater.algorithm();
    let new_gas_price = algo.calculate();
    assert!(
        new_gas_price > old_gas_price,
        "{} !> {}",
        new_gas_price,
        old_gas_price
    );
}