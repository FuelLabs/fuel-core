use super::*;

#[test]
fn calculate__even_profit_maintains_price() {
    // given
    let starting_exec_gas_price = 100;
    let starting_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_gas_price_denominator = 1;
    let block_bytes = 500;
    let starting_reward = starting_cost + block_bytes * latest_gas_per_byte;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(starting_da_gas_price)
        .with_da_p_component(da_gas_price_denominator)
        .with_total_rewards(starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let expected = starting_exec_gas_price + starting_da_gas_price;
    assert_eq!(expected, actual);
}

#[test]
fn calculate__negative_profit_increase_gas_price() {
    // given
    let starting_exec_gas_price = 100;
    let last_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_p_component = 100;
    let da_d_component = 10;
    let block_bytes = 500;
    let latest_profit = 100;
    let arb_value = 1000;
    let smaller_starting_reward =
        starting_cost + block_bytes * latest_gas_per_byte - arb_value;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_total_rewards(smaller_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .with_last_profit(latest_profit)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    // let profit = (smaller_starting_reward as i64
    //     - (starting_cost + block_bytes * latest_gas_per_byte) as i64);
    // let expected = starting_exec_gas_price
    //     + last_da_gas_price
    //     + (profit.abs() / da_p_component) as u64;
    // assert_eq!(expected, actual);

    let profit = smaller_starting_reward as i64
        - (starting_cost + block_bytes * latest_gas_per_byte) as i64;
    let da_p_comp = (profit / da_p_component);
    let slope = profit - latest_profit;
    let da_d_comp = (slope / da_d_component);
    let expected =
        starting_exec_gas_price as i64 + last_da_gas_price as i64 - da_p_comp - da_d_comp;
    assert_eq!(expected as u64, actual);
}

#[test]
fn calculate__positive_profit_decrease_gas_price() {
    // given
    let starting_exec_gas_price = 100;
    let last_da_gas_price = 100;
    let starting_cost = 500;
    let latest_gas_per_byte = 10;
    let da_p_component = 100;
    let da_d_component = 10;
    let block_bytes = 500;
    let latest_profit = 100;
    let arb_value = 1000;
    let larger_starting_reward =
        starting_cost + block_bytes * latest_gas_per_byte + arb_value;
    let mut updater = UpdaterBuilder::new()
        .with_starting_exec_gas_price(starting_exec_gas_price)
        .with_da_p_component(da_p_component)
        .with_da_d_component(da_d_component)
        .with_starting_da_gas_price(last_da_gas_price)
        .with_total_rewards(larger_starting_reward)
        .with_known_total_cost(starting_cost)
        .with_projected_total_cost(starting_cost)
        .with_da_cost_per_byte(latest_gas_per_byte)
        .with_last_profit(latest_profit)
        .build();

    // when
    let algo = updater.algorithm();
    let actual = algo.calculate(block_bytes);

    // then
    let profit = larger_starting_reward as i64
        - (starting_cost + block_bytes * latest_gas_per_byte) as i64;
    let slope = profit - latest_profit;
    let da_p_comp = (profit / da_p_component);
    let da_d_comp = (slope / da_d_component);
    let expected =
        starting_exec_gas_price as i64 + last_da_gas_price as i64 - da_p_comp - da_d_comp;
    assert_eq!(expected as u64, actual);
}
