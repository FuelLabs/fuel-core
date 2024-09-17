#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

use crate::fuel_gas_price_updater::{
    da_source_adapter::service::{
        new_provider,
        DaBlockCostsProvider,
        DaBlockCostsProviderPort,
        DaBlockCostsSource,
    },
    BlockInfo,
    DaBlockCosts,
    L2BlockSource,
};
use async_trait::async_trait;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
};
use fuel_core_types::fuel_types::BlockHeight;
use futures::FutureExt;
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod static_updater;

pub mod fuel_gas_price_updater;

/// The service that updates the gas price algorithm.
pub struct GasPriceService<A, U, L2, DA: DaBlockCostsSource + 'static> {
    /// The algorithm that can be used in the next block
    next_block_algorithm: SharedGasPriceAlgo<A>,
    /// The code that is run to update your specific algorithm
    update_algorithm: U,
    /// The L2 block source
    l2_block_source: L2,
    /// The DA block costs provider
    da_block_costs_provider: DaBlockCostsProvider<DA>,
    /// The cached value of da block costs
    da_block_costs: Option<DaBlockCosts>,
}

impl<A, U, L2, DA> GasPriceService<A, U, L2, DA>
where
    U: UpdateAlgorithm<Algorithm = A>,
    A: Send + Sync,
    DA: DaBlockCostsSource,
{
    pub async fn new(
        starting_block_height: BlockHeight,
        update_algorithm: U,
        mut shared_algo: SharedGasPriceAlgo<A>,
        l2_block_source: L2,
        da_block_costs_source: DA,
    ) -> Self {
        let algorithm = update_algorithm.start(starting_block_height);
        shared_algo.update(algorithm).await;

        let da_block_costs_provider = new_provider(da_block_costs_source, None);
        Self {
            next_block_algorithm: shared_algo,
            update_algorithm,
            l2_block_source,
            da_block_costs: None,
            da_block_costs_provider,
        }
    }

    pub fn next_block_algorithm(&self) -> SharedGasPriceAlgo<A> {
        self.next_block_algorithm.clone()
    }
}

/// The interface the Service has with the code that updates the gas price algorithm.
#[async_trait]
pub trait UpdateAlgorithm {
    /// The type of the algorithm that is being updated
    type Algorithm;

    /// Start the algorithm at a given block height
    fn start(&self, for_block: BlockHeight) -> Self::Algorithm;

    /// Wait for the next algorithm to be available
    async fn next(
        &mut self,
        l2_block: BlockInfo,
        da_block_costs: Option<DaBlockCosts>,
    ) -> anyhow::Result<Self::Algorithm>;
}

pub trait GasPriceAlgorithm {
    fn next_gas_price(&self) -> u64;
    fn worst_case_gas_price(&self, block_height: BlockHeight) -> u64;
}

impl<A, U, L2, DA> GasPriceService<A, U, L2, DA>
where
    U: UpdateAlgorithm<Algorithm = A>,
    A: Send + Sync,
    DA: DaBlockCostsSource,
{
    async fn update(&mut self, new_algorithm: A) {
        self.next_block_algorithm.update(new_algorithm).await;
    }
}

#[derive(Debug, Default)]
pub struct SharedGasPriceAlgo<A>(Arc<RwLock<A>>);

impl<A> Clone for SharedGasPriceAlgo<A> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<A> SharedGasPriceAlgo<A>
where
    A: Send + Sync,
{
    pub fn new_with_algorithm(algorithm: A) -> Self {
        Self(Arc::new(RwLock::new(algorithm)))
    }

    pub async fn update(&mut self, new_algo: A) {
        let mut write_lock = self.0.write().await;
        *write_lock = new_algo;
    }
}

impl<A> SharedGasPriceAlgo<A>
where
    A: GasPriceAlgorithm + Send + Sync,
{
    pub async fn next_gas_price(&self) -> u64 {
        self.0.read().await.next_gas_price()
    }

    pub async fn worst_case_gas_price(&self, block_height: BlockHeight) -> u64 {
        self.0.read().await.worst_case_gas_price(block_height)
    }
}

#[async_trait]
impl<A, U, L2, DA> RunnableService for GasPriceService<A, U, L2, DA>
where
    U: UpdateAlgorithm<Algorithm = A> + Send + Sync,
    A: Send + Sync,
    L2: L2BlockSource,
    DA: DaBlockCostsSource,
{
    const NAME: &'static str = "GasPriceUpdater";
    type SharedData = SharedGasPriceAlgo<A>;
    type Task = Self;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.next_block_algorithm.clone()
    }

    async fn into_task(
        self,
        _state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        self.da_block_costs_provider.start_and_await().await?;
        Ok(self)
    }
}

#[async_trait]
impl<A, U, L2, DA> RunnableTask for GasPriceService<A, U, L2, DA>
where
    U: UpdateAlgorithm<Algorithm = A> + Send + Sync,
    A: Send + Sync,
    L2: L2BlockSource,
    DA: DaBlockCostsSource,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                tracing::debug!("Stopping gas price service");
                should_continue = false;
            }
            da_block_costs = self.da_block_costs_provider.recv() => {
                if da_block_costs.is_err() {
                    tracing::error!("Failed to get da block costs: {:?}", da_block_costs.err());
                } else {
                    self.da_block_costs = Some(da_block_costs?);
                }
                should_continue = true;
            }
            l2_block = self.l2_block_source.get_l2_block() => {
                let l2_block = l2_block?;
                let da_block_costs = self.da_block_costs.take();
                let next_algo = self.update_algorithm.next(l2_block, da_block_costs).await?;
                tracing::debug!("Updating gas price algorithm");
                self.update(next_algo).await;
                should_continue = true;
            }
        }
        Ok(should_continue)
    }

    async fn shutdown(mut self) -> anyhow::Result<()> {
        self.da_block_costs_provider.stop_and_await().await?;

        let l2_block = self.l2_block_source.get_l2_block().now_or_never();
        if let Some(Ok(l2_block)) = l2_block {
            let da_block_costs = self.da_block_costs.clone();
            if let Some(new_algo) = self
                .update_algorithm
                .next(l2_block, da_block_costs)
                .now_or_never()
            {
                let new_algo = new_algo?;
                tracing::debug!("Updating gas price algorithm");
                self.update(new_algo).await;
            }
        }
        Ok(())
    }
}

#[allow(clippy::arithmetic_side_effects)]
#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use crate::{
        fuel_gas_price_updater,
        fuel_gas_price_updater::{
            da_source_adapter::dummy_costs::DummyDaBlockCosts,
            BlockInfo,
            DaBlockCosts,
            L2BlockSource,
        },
        GasPriceAlgorithm,
        GasPriceService,
        SharedGasPriceAlgo,
        UpdateAlgorithm,
    };
    use fuel_core_services::{
        Service,
        ServiceRunner,
    };
    use fuel_core_types::fuel_types::BlockHeight;
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[derive(Clone, Debug)]
    struct TestAlgorithm {
        price: u64,
    }

    impl GasPriceAlgorithm for TestAlgorithm {
        fn next_gas_price(&self) -> u64 {
            self.price
        }

        fn worst_case_gas_price(&self, _block_height: BlockHeight) -> u64 {
            self.price
        }
    }

    struct TestAlgorithmUpdater {
        start: TestAlgorithm,
        price_source: mpsc::Receiver<u64>,
    }

    #[async_trait::async_trait]
    impl UpdateAlgorithm for TestAlgorithmUpdater {
        type Algorithm = TestAlgorithm;

        fn start(&self, _for_block: BlockHeight) -> Self::Algorithm {
            self.start.clone()
        }

        async fn next(
            &mut self,
            _l2_block: BlockInfo,
            _da_block_costs: Option<DaBlockCosts>,
        ) -> anyhow::Result<Self::Algorithm> {
            let price = self.price_source.recv().await.unwrap();
            Ok(TestAlgorithm { price })
        }
    }

    struct FakeL2BlockSource;

    #[async_trait::async_trait]
    impl L2BlockSource for FakeL2BlockSource {
        async fn get_l2_block(&mut self) -> fuel_gas_price_updater::Result<BlockInfo> {
            // simulate fetch
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(BlockInfo::Block {
                height: 0,
                gas_used: 0,
                block_gas_capacity: 0,
            })
        }
    }

    #[tokio::test]
    async fn run__updates_gas_price() {
        // given
        let (price_sender, price_receiver) = mpsc::channel(1);
        let start_algo = TestAlgorithm { price: 50 };
        let expected_price = 100;
        let updater = TestAlgorithmUpdater {
            start: TestAlgorithm {
                price: expected_price,
            },
            price_source: price_receiver,
        };
        let shared_algo = SharedGasPriceAlgo::new_with_algorithm(start_algo);
        let l2_block_source = FakeL2BlockSource;
        let da_block_source =
            DummyDaBlockCosts::new(Err(anyhow::anyhow!("not implemented")));
        let service = GasPriceService::new(
            0.into(),
            updater,
            shared_algo,
            l2_block_source,
            da_block_source,
        )
        .await;
        let read_algo = service.next_block_algorithm();
        let service = ServiceRunner::new(service);
        service.start_and_await().await.unwrap();

        // when
        price_sender.send(expected_price).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;

        // then
        let actual_price = read_algo.next_gas_price().await;
        assert_eq!(expected_price, actual_price);
        service.stop_and_await().await.unwrap();
    }
}
