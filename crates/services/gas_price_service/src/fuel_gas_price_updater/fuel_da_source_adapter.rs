use crate::fuel_gas_price_updater::{
    DaBlockCosts,
    Error::CouldNotFetchDARecord,
    GetDaBlockCosts,
    Result as GasPriceUpdaterResult,
};
use anyhow::anyhow;
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod block_committer_costs;
pub mod dummy_costs;
pub mod service;

pub use block_committer_costs::BlockCommitterDaBlockCosts;
pub use dummy_costs::DummyDaBlockCosts;
pub use service::*;

pub const POLLING_INTERVAL_MS: u64 = 10_000;

pub type DaBlockCostsProvider = Arc<Mutex<Option<DaBlockCosts>>>;

impl GetDaBlockCosts for DaBlockCostsProvider {
    fn get(&mut self) -> GasPriceUpdaterResult<Option<DaBlockCosts>> {
        let mut da_block_costs_guard = self.try_lock().map_err(|err| {
            CouldNotFetchDARecord(anyhow!(
                "Failed to lock shared gas price state: {:?}",
                err
            ))
        })?;

        let da_block_costs = da_block_costs_guard.clone();

        // now mark it as consumed because we don't want to serve the same data
        // multiple times
        *da_block_costs_guard = None;

        Ok(da_block_costs)
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_gas_price_updater::{
        fuel_da_source_adapter::service::Result as DaBlockCostsResult,
        DaBlockCostsService,
        DaBlockCostsSource,
        DummyDaBlockCosts,
    };
    use fuel_core_services::{
        RunnableService,
        Service,
        ServiceRunner,
    };
    use std::time::Duration;
    use tokio::time::sleep;

    #[derive(Default)]
    struct ErroringSource;

    #[async_trait::async_trait]
    impl DaBlockCostsSource for ErroringSource {
        async fn get(&mut self) -> DaBlockCostsResult<DaBlockCosts> {
            Err(anyhow!("boo!"))
        }
    }

    #[tokio::test]
    async fn run__when_da_block_cost_source_gives_value_shared_value_is_updated() {
        // given
        let service =
            DaBlockCostsService::new(DummyDaBlockCosts, Some(Duration::from_millis(1)));

        let mut shared_state = service.shared_data();
        let service = ServiceRunner::new(service);

        // when
        service.start().unwrap();
        sleep(Duration::from_millis(10)).await;
        service.stop();

        // then
        let da_block_costs_opt = shared_state.get().unwrap();
        assert!(da_block_costs_opt.is_some());
    }

    #[tokio::test]
    async fn run__when_da_block_cost_source_gives_value_shared_value_is_marked_stale() {
        // given
        let service =
            DaBlockCostsService::new(DummyDaBlockCosts, Some(Duration::from_millis(1)));
        let mut shared_state = service.shared_data();
        let service = ServiceRunner::new(service);

        // when
        service.start().unwrap();
        sleep(Duration::from_millis(10)).await;
        service.stop();
        let _ = shared_state.get().unwrap();

        // then
        let da_block_costs_opt = shared_state.get().unwrap();
        assert!(da_block_costs_opt.is_none());
    }

    #[tokio::test]
    async fn run__when_da_block_cost_source_errors_shared_value_is_not_updated() {
        // given
        let service =
            DaBlockCostsService::new(ErroringSource, Some(Duration::from_millis(1)));
        let mut shared_state = service.shared_data();
        let service = ServiceRunner::new(service);

        // when
        service.start().unwrap();
        sleep(Duration::from_millis(10)).await;
        service.stop();

        // then
        let da_block_costs_opt = shared_state.get().unwrap();
        assert!(da_block_costs_opt.is_none());
    }
}
