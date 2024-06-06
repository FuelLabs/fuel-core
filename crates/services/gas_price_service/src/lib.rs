use async_trait::async_trait;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_types::fuel_types::BlockHeight;
use std::sync::Arc;

use tokio::sync::{
    RwLock,
    RwLockReadGuard,
};

pub mod static_updater;

pub fn new_service<A, U>(
    current_fuel_block_height: BlockHeight,
    update_algo: U,
) -> anyhow::Result<ServiceRunner<GasPriceService<A, U>>>
where
    U: UpdateAlgorithm<Algorithm = A> + Send + Sync,
    A: Send + Sync,
{
    let service = GasPriceService::new(current_fuel_block_height, update_algo);
    Ok(ServiceRunner::new(service))
}

pub struct GasPriceService<A, U> {
    next_block_algorithm: BlockGasPriceAlgo<A>,
    update_algorithm: U,
}

impl<A, U> GasPriceService<A, U>
where
    U: UpdateAlgorithm<Algorithm = A>,
    A: Send + Sync,
{
    pub fn new(starting_block_height: BlockHeight, update_algorithm: U) -> Self {
        let algorithm = update_algorithm.start(starting_block_height);
        let next_block_algorithm =
            BlockGasPriceAlgo::new(starting_block_height, algorithm);
        Self {
            next_block_algorithm,
            update_algorithm,
        }
    }

    pub fn next_block_algorithm(&self) -> BlockGasPriceAlgo<A> {
        self.next_block_algorithm.clone()
    }
}

#[async_trait]
pub trait UpdateAlgorithm {
    type Algorithm;

    fn start(&self, for_block: BlockHeight) -> Self::Algorithm;
    async fn next(&mut self, for_block: BlockHeight) -> Self::Algorithm;
}

fn next_block_height(block_height: &BlockHeight) -> BlockHeight {
    (u32::from(*block_height) + 1).into()
}

impl<A, U> GasPriceService<A, U>
where
    U: UpdateAlgorithm<Algorithm = A>,
    A: Send + Sync,
{
    async fn update(&mut self, new_block_height: BlockHeight, new_algorithm: A) {
        self.next_block_algorithm
            .update(new_block_height, new_algorithm)
            .await;
    }

    async fn latest_block_height(&self) -> BlockHeight {
        self.next_block_algorithm.read().await.0
    }
}

#[derive(Debug)]
pub struct BlockGasPriceAlgo<A>(Arc<RwLock<(BlockHeight, A)>>);

impl<A> Clone for BlockGasPriceAlgo<A> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<A> BlockGasPriceAlgo<A>
where
    A: Send + Sync,
{
    pub fn new(block_height: BlockHeight, algo: A) -> Self {
        Self(Arc::new(RwLock::new((block_height, algo))))
    }

    pub async fn block_height(&self) -> BlockHeight {
        let lock = self.0.read().await;
        lock.0.clone()
    }

    pub async fn read(&self) -> RwLockReadGuard<(BlockHeight, A)> {
        self.0.read().await
    }

    pub async fn update(&mut self, new_block_height: BlockHeight, new_algo: A) {
        let mut write_lock = self.0.write().await;
        write_lock.0 = new_block_height;
        write_lock.1 = new_algo;
    }
}

#[async_trait]
impl<A, U> RunnableService for GasPriceService<A, U>
where
    U: UpdateAlgorithm<Algorithm = A> + Send + Sync,
    A: Send + Sync,
{
    const NAME: &'static str = "GasPriceUpdater";
    type SharedData = BlockGasPriceAlgo<A>;
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
        Ok(self)
    }
}

#[async_trait]
impl<A, U> RunnableTask for GasPriceService<A, U>
where
    U: UpdateAlgorithm<Algorithm = A> + Send + Sync,
    A: Send + Sync,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        let latest_block = self.latest_block_height().await;
        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                tracing::debug!("Stopping gas price service");
                should_continue = false;
            }
            new_algo = self.update_algorithm.next(latest_block) => {
                tracing::debug!("Updating gas price algorithm");
                self.update(next_block_height(&latest_block), new_algo).await;
                should_continue = true;
            }
        }
        Ok(should_continue)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use crate::{
        GasPriceService,
        UpdateAlgorithm,
    };
    use fuel_core_services::{
        RunnableService,
        RunnableTask,
        Service,
        ServiceRunner,
        State,
        StateWatcher,
    };
    use fuel_core_types::fuel_types::BlockHeight;
    use tokio::sync::mpsc;

    #[derive(Clone, Debug)]
    struct TestAlgorithm {
        price: u64,
    }

    impl TestAlgorithm {
        fn gas_price(&self) -> u64 {
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

        async fn next(&mut self, _for_block: BlockHeight) -> Self::Algorithm {
            let price = self.price_source.recv().await.unwrap();
            TestAlgorithm { price }
        }
    }
    #[tokio::test]
    async fn run__updates_gas_price() {
        let _ = tracing_subscriber::fmt::try_init();

        // given
        let (price_sender, price_receiver) = mpsc::channel(1);
        let updater = TestAlgorithmUpdater {
            start: TestAlgorithm { price: 0 },
            price_source: price_receiver,
        };
        let service = GasPriceService::new(0.into(), updater);
        let (watch_sender, watch_receiver) = tokio::sync::watch::channel(State::Started);
        let watcher = StateWatcher::from(watch_receiver);
        let read_algo = service.next_block_algorithm();
        let task = service.into_task(&watcher, ()).await.unwrap();
        let expected_price = 100;
        let service = ServiceRunner::new(task);
        service.start().unwrap();

        // when
        price_sender.send(expected_price).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // then
        let actual_price = read_algo.read().unwrap().1.gas_price();
        assert_eq!(expected_price, actual_price);

        watch_sender.send(State::Stopped).unwrap();
    }
}
