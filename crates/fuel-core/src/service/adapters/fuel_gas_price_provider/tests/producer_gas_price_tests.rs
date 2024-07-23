use crate::service::adapters::fuel_gas_price_provider::tests::build_provider;
use fuel_core_gas_price_service::{
    static_updater::StaticAlgorithm,
    GasPriceAlgorithm,
};

#[tokio::test]
async fn gas_price__if_requested_block_height_is_latest_return_gas_price() {
    // given
    let price = 33;
    let algo = StaticAlgorithm::new(price);
    let gas_price_provider = build_provider(algo.clone());

    // when
    let expected_price = algo.next_gas_price();
    let actual_price = gas_price_provider.next_gas_price().await;

    // then
    assert_eq!(expected_price, actual_price);
}
