pub mod block_committer_costs;
pub mod dummy_costs;
pub mod service;

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use crate::fuel_gas_price_updater::{
        da_source_adapter::{
            dummy_costs::DummyDaBlockCosts,
            service::{
                new_provider,
                DaBlockCostsProviderPort,
            },
        },
        DaBlockCosts,
    };
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn run__when_da_block_cost_source_gives_value_shared_value_is_updated() {
        // given
        let expected_da_cost = DaBlockCosts {
            l2_block_range: 0..10,
            blob_size_bytes: 1024 * 128,
            blob_cost_wei: 2,
        };
        let da_block_costs_source = DummyDaBlockCosts::new(Ok(expected_da_cost.clone()));
        let mut provider =
            new_provider(da_block_costs_source, Some(Duration::from_millis(1)));

        // when
        provider.start_and_await().await.unwrap();
        sleep(Duration::from_millis(10)).await;
        provider.stop_and_await().await.unwrap();

        // then
        let da_block_costs = provider.recv().await.unwrap();
        assert_eq!(da_block_costs, expected_da_cost);
    }

    #[tokio::test]
    async fn run__when_da_block_cost_source_gives_value_shared_value_is_marked_stale() {
        // given
        let expected_da_cost = DaBlockCosts {
            l2_block_range: 0..10,
            blob_size_bytes: 1024 * 128,
            blob_cost_wei: 2,
        };
        let da_block_costs_source = DummyDaBlockCosts::new(Ok(expected_da_cost.clone()));
        let mut provider =
            new_provider(da_block_costs_source, Some(Duration::from_millis(1)));

        // when
        provider.start_and_await().await.unwrap();
        sleep(Duration::from_millis(10)).await;
        provider.stop_and_await().await.unwrap();
        let da_block_costs = provider.recv().await.unwrap();
        assert_eq!(da_block_costs, expected_da_cost);

        // then
        let da_block_costs_opt = provider.try_recv();
        assert!(da_block_costs_opt.is_err());
    }

    #[tokio::test]
    async fn run__when_da_block_cost_source_errors_shared_value_is_not_updated() {
        // given
        let da_block_costs_source = DummyDaBlockCosts::new(Err(anyhow::anyhow!("boo!")));
        let mut provider =
            new_provider(da_block_costs_source, Some(Duration::from_millis(1)));

        // when
        provider.start_and_await().await.unwrap();
        sleep(Duration::from_millis(10)).await;
        provider.stop_and_await().await.unwrap();

        // then
        let da_block_costs_opt = provider.try_recv();
        assert!(da_block_costs_opt.is_err());
    }
}
