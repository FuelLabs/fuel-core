use crate::v1::tests::UpdaterBuilder;

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
