use crate::v1::da_source_service::service::DaBlockCostsSource;
use std::time::Duration;

pub mod block_committer_costs;
pub mod dummy_costs;
pub mod service;

#[derive(Debug, Default, Clone, Eq, Hash, PartialEq)]
pub struct DaBlockCosts {
    pub l2_block_range: core::ops::Range<u64>,
    pub blob_size_bytes: u32,
    pub blob_cost_wei: u128,
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::da_source_service::{
        dummy_costs::DummyDaBlockCosts,
        service::new_service,
    };
    use fuel_core_services::Service;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    #[ignore]
    async fn run__when_da_block_cost_source_gives_value_shared_state_is_updated() {
        // given
        let expected_da_cost = DaBlockCosts {
            l2_block_range: 0..10,
            blob_size_bytes: 1024 * 128,
            blob_cost_wei: 2,
        };
        let da_block_costs_source = DummyDaBlockCosts::new(Ok(expected_da_cost.clone()));
        let service = new_service(da_block_costs_source, Some(Duration::from_millis(1)));
        let mut shared_state = &mut service.shared.subscribe();

        // when
        service.start_and_await().await.unwrap();
        sleep(Duration::from_millis(10)).await;
        service.stop_and_await().await.unwrap();

        // then
        let da_block_costs = shared_state.try_recv().unwrap();
        assert_eq!(da_block_costs, expected_da_cost);
    }

    #[tokio::test]
    #[ignore]
    async fn run__when_da_block_cost_source_errors_shared_state_is_not_updated() {
        // given
        let da_block_costs_source = DummyDaBlockCosts::new(Err(anyhow::anyhow!("boo!")));
        let service = new_service(da_block_costs_source, Some(Duration::from_millis(1)));
        let mut shared_state = &mut service.shared.subscribe();

        // when
        service.start_and_await().await.unwrap();
        sleep(Duration::from_millis(10)).await;
        service.stop_and_await().await.unwrap();

        // then
        let da_block_costs_res = shared_state.try_recv();
        assert!(da_block_costs_res.is_err());
        assert!(matches!(
            da_block_costs_res.err().unwrap(),
            tokio::sync::broadcast::error::TryRecvError::Empty
        ));
    }
}
