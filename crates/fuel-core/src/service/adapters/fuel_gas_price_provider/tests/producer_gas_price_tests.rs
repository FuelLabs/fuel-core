use crate::service::adapters::fuel_gas_price_provider::tests::build_provider;
use fuel_core_gas_price_service::{
    common::gas_price_algorithm::GasPriceAlgorithm,
    static_updater::StaticAlgorithm,
};
use fuel_core_producer::block_producer::gas_price::GasPriceProvider;
use fuel_core_types::clamped_percentage::ClampedPercentage;

#[test]
fn production_gas_price__if_requested_block_height_is_latest_return_gas_price() {
    // given
    let price = 33;
    let algo = StaticAlgorithm::new(price);
    let gas_price_provider =
        build_provider(algo.clone(), 0, price, ClampedPercentage::new(10));

    // when
    let expected_price = algo.next_gas_price();
    let actual_price = gas_price_provider.production_gas_price().unwrap();

    // then
    assert_eq!(expected_price, actual_price);
}

#[test]
fn dry_run_gas_price__calculates_correctly_based_on_percentage() {
    // given
    let height = 123;
    let price = 33;
    let percentage = ClampedPercentage::new(10);
    let algo = StaticAlgorithm::new(price);
    let gas_price_provider = build_provider(algo.clone(), height, price, percentage);

    // when
    let actual = gas_price_provider.dry_run_gas_price().unwrap();

    // then
    let change_amount = price.saturating_mul(*percentage as u64).saturating_div(100);
    let expected = price + change_amount;
    assert_eq!(expected, actual);
}
