use crate::fuel_gas_price_updater::{
    da_source_adapter::service::{
        new_service,
        DaBlockCostsService,
        DaBlockCostsSource,
    },
    DaBlockCosts,
    GetDaBlockCosts,
    Result as GasPriceUpdaterResult,
};
use fuel_core_services::ServiceRunner;
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::sync::{
    mpsc,
    Mutex,
};

pub mod block_committer_costs;
pub mod dummy_costs;
pub mod service;

pub const POLLING_INTERVAL_MS: u64 = 10_000;

#[derive(Clone)]
pub struct DaBlockCostsSharedState {
    inner: Arc<Mutex<mpsc::Receiver<DaBlockCosts>>>,
}

impl DaBlockCostsSharedState {
    fn new(receiver: mpsc::Receiver<DaBlockCosts>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(receiver)),
        }
    }
}

pub struct DaBlockCostsProvider<T: DaBlockCostsSource + 'static> {
    pub service: ServiceRunner<DaBlockCostsService<T>>,
    pub shared_state: DaBlockCostsSharedState,
}

const CHANNEL_BUFFER_SIZE: usize = 10;

impl<T> DaBlockCostsProvider<T>
where
    T: DaBlockCostsSource,
{
    pub fn new(source: T, polling_interval: Option<Duration>) -> Self {
        let (sender, receiver) = mpsc::channel(CHANNEL_BUFFER_SIZE);
        let service = new_service(source, sender, polling_interval);
        Self {
            shared_state: DaBlockCostsSharedState::new(receiver),
            service,
        }
    }
}

impl GetDaBlockCosts for DaBlockCostsSharedState {
    fn get(&self) -> GasPriceUpdaterResult<Option<DaBlockCosts>> {
        if let Ok(mut guard) = self.inner.try_lock() {
            if let Ok(da_block_costs) = guard.try_recv() {
                return Ok(Some(da_block_costs));
            }
        }
        Ok(None)
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_gas_price_updater::da_source_adapter::dummy_costs::DummyDaBlockCosts;
    use fuel_core_services::Service;
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
        let provider = DaBlockCostsProvider::new(
            da_block_costs_source,
            Some(Duration::from_millis(1)),
        );
        let shared_state = provider.shared_state.clone();

        // when
        provider.service.start_and_await().await.unwrap();
        sleep(Duration::from_millis(10)).await;
        provider.service.stop_and_await().await.unwrap();

        // then
        let da_block_costs_opt = shared_state.get().unwrap();
        assert!(da_block_costs_opt.is_some());
        assert_eq!(da_block_costs_opt.unwrap(), expected_da_cost);
    }

    #[tokio::test]
    async fn run__when_da_block_cost_source_gives_value_shared_value_is_marked_stale() {
        // given
        let expected_da_cost = DaBlockCosts {
            l2_block_range: 0..10,
            blob_size_bytes: 1024 * 128,
            blob_cost_wei: 1,
        };
        let da_block_costs_source = DummyDaBlockCosts::new(Ok(expected_da_cost.clone()));
        let provider = DaBlockCostsProvider::new(
            da_block_costs_source,
            Some(Duration::from_millis(1)),
        );
        let shared_state = provider.shared_state.clone();

        // when
        provider.service.start_and_await().await.unwrap();
        sleep(Duration::from_millis(10)).await;
        provider.service.stop_and_await().await.unwrap();
        let da_block_costs_opt = shared_state.get().unwrap();
        assert!(da_block_costs_opt.is_some());
        assert_eq!(da_block_costs_opt.unwrap(), expected_da_cost);

        // then
        let da_block_costs_opt = shared_state.get().unwrap();
        assert!(da_block_costs_opt.is_none());
    }

    #[tokio::test]
    async fn run__when_da_block_cost_source_errors_shared_value_is_not_updated() {
        // given
        let da_block_costs_source = DummyDaBlockCosts::new(Err(anyhow::anyhow!("boo!")));
        let provider = DaBlockCostsProvider::new(
            da_block_costs_source,
            Some(Duration::from_millis(1)),
        );
        let shared_state = provider.shared_state.clone();

        // when
        provider.service.start_and_await().await.unwrap();
        sleep(Duration::from_millis(10)).await;
        provider.service.stop_and_await().await.unwrap();

        // then
        let da_block_costs_opt = shared_state.get().unwrap();
        assert!(da_block_costs_opt.is_none());
    }
}
