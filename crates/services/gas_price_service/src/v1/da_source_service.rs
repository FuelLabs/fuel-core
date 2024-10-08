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

    #[tokio::test]
    async fn run__when_da_block_cost_source_gives_value_shared_state_is_updated() {
        // given
        let expected_da_cost = DaBlockCosts {
            l2_block_range: 0..10,
            blob_size_bytes: 1024 * 128,
            blob_cost_wei: 2,
        };
        let (notifier_sender, mut notifier_receiver) = tokio::sync::mpsc::channel(1);
        let da_block_costs_source =
            DummyDaBlockCosts::new(Ok(expected_da_cost.clone()), notifier_sender);
        let service = new_service(da_block_costs_source, Some(Duration::from_millis(1)));
        let mut shared_state = &mut service.shared.subscribe();

        // when
        service.start_and_await().await.unwrap();

        // then
        let result = notifier_receiver.recv().await.unwrap();
        assert!(result);
        let shared_state_value = shared_state.recv().await.unwrap();
        assert_eq!(shared_state_value, expected_da_cost);
        service.stop_and_await().await.unwrap();
    }

    #[tokio::test]
    async fn run__when_da_block_cost_source_errors_shared_state_is_not_updated() {
        // given
        let (notifier_sender, mut notifier_receiver) = tokio::sync::mpsc::channel(1);
        let da_block_costs_source =
            DummyDaBlockCosts::new(Err(anyhow::anyhow!("boo!")), notifier_sender);
        let service = new_service(da_block_costs_source, Some(Duration::from_millis(1)));
        let mut shared_state = &mut service.shared.subscribe();

        // when
        service.start_and_await().await.unwrap();

        // then
        let result = notifier_receiver.recv().await.unwrap();
        assert!(!result);
        let shared_state_value = shared_state.try_recv();
        assert!(shared_state_value.is_err());
        service.stop_and_await().await.unwrap();
    }
}
