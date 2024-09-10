use crate::fuel_gas_price_updater::{
    DaBlockCosts,
    GetDaBlockCosts,
    Result as GasPriceUpdaterResult,
};
use std::sync::Arc;
use tokio::sync::{
    mpsc,
    Mutex,
};

pub mod block_committer_costs;
pub mod dummy_costs;
pub mod service;

pub const POLLING_INTERVAL_MS: u64 = 10_000;

#[derive(Clone)]
pub struct DaBlockCostsProvider {
    receiver: Arc<Mutex<mpsc::Receiver<DaBlockCosts>>>,
}

impl DaBlockCostsProvider {
    fn from_receiver(receiver: mpsc::Receiver<DaBlockCosts>) -> Self {
        Self {
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }
}

impl GetDaBlockCosts for DaBlockCostsProvider {
    fn get(&mut self) -> GasPriceUpdaterResult<Option<DaBlockCosts>> {
        if let Ok(mut guard) = self.receiver.try_lock() {
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
    use crate::fuel_gas_price_updater::da_source_adapter::{
        dummy_costs::DummyDaBlockCosts,
        service::{
            new_service,
            DaBlockCostsSource,
            Result as DaBlockCostsResult,
        },
    };
    use fuel_core_services::Service;
    use std::time::Duration;
    use tokio::time::sleep;

    #[derive(Default)]
    struct ErroringSource;

    #[async_trait::async_trait]
    impl DaBlockCostsSource for ErroringSource {
        async fn request_da_block_cost(&mut self) -> DaBlockCostsResult<DaBlockCosts> {
            Err(anyhow::anyhow!("boo!"))
        }
    }

    #[tokio::test]
    async fn run__when_da_block_cost_source_gives_value_shared_value_is_updated() {
        // given
        let service = new_service(DummyDaBlockCosts, Some(Duration::from_millis(1)));
        let mut shared_state = service.shared.clone();

        // when
        service.start_and_await().await.unwrap();
        sleep(Duration::from_millis(10)).await;
        service.stop_and_await().await.unwrap();

        // then
        let da_block_costs_opt = shared_state.get().unwrap();
        assert!(da_block_costs_opt.is_some());
    }

    #[tokio::test]
    async fn run__when_da_block_cost_source_gives_value_shared_value_is_marked_stale() {
        // given
        let service = new_service(DummyDaBlockCosts, Some(Duration::from_millis(1)));
        let mut shared_state = service.shared.clone();

        // when
        service.start_and_await().await.unwrap();
        sleep(Duration::from_millis(10)).await;
        service.stop_and_await().await.unwrap();
        let _ = shared_state.get().unwrap();

        // then
        let da_block_costs_opt = shared_state.get().unwrap();
        assert!(da_block_costs_opt.is_none());
    }

    #[tokio::test]
    async fn run__when_da_block_cost_source_errors_shared_value_is_not_updated() {
        // given
        let service = new_service(ErroringSource, Some(Duration::from_millis(1)));
        let mut shared_state = service.shared.clone();

        // when
        service.start_and_await().await.unwrap();
        sleep(Duration::from_millis(10)).await;
        service.stop_and_await().await.unwrap();

        // then
        let da_block_costs_opt = shared_state.get().unwrap();
        assert!(da_block_costs_opt.is_none());
    }
}
