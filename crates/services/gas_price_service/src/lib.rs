use async_trait::async_trait;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_types::fuel_types::BlockHeight;
use std::sync::{
    Arc,
    RwLock,
};

pub mod static_updater;

pub fn new_service<A, U>(
    current_fuel_block_height: BlockHeight,
    update_algo: U,
) -> anyhow::Result<ServiceRunner<GasPriceService<A, U>>>
where
    U: UpdateAlgorithm<Algorithm = A> + Send,
    A: Send + Sync,
{
    let service = GasPriceService::new(current_fuel_block_height, update_algo);
    Ok(ServiceRunner::new(service))
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to update gas price algorithm")]
    UpdateFailed(String),
}

pub struct GasPriceService<A, U> {
    next_block_algorithm: Arc<RwLock<(BlockHeight, A)>>,
    update_algorithm: U,
}

impl<A, U> GasPriceService<A, U>
where
    U: UpdateAlgorithm<Algorithm = A>,
{
    pub fn new(starting_block_height: BlockHeight, update_algorithm: U) -> Self {
        let algorithm = update_algorithm.start(starting_block_height);
        let next_block_algorithm =
            Arc::new(RwLock::new((starting_block_height, algorithm)));
        Self {
            next_block_algorithm,
            update_algorithm,
        }
    }

    pub fn next_block_algorithm(&self) -> Arc<RwLock<(BlockHeight, A)>> {
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
{
    fn update(
        &self,
        new_block_height: BlockHeight,
        new_algorithm: A,
    ) -> Result<(), Error> {
        let mut latest_algorithm = self
            .next_block_algorithm
            .write()
            .map_err(|e| Error::UpdateFailed(format!("{e:?}")))?;
        let (block_height, algorithm) = &mut *latest_algorithm;
        *algorithm = new_algorithm;
        *block_height = new_block_height;
        Ok(())
    }

    fn latest_block_height(&self) -> BlockHeight {
        self.next_block_algorithm.read().unwrap().0
    }
}

#[async_trait]
impl<A, U> RunnableService for GasPriceService<A, U>
where
    U: UpdateAlgorithm<Algorithm = A> + Send,
    A: Send + Sync,
{
    const NAME: &'static str = "GasPriceUpdater";
    type SharedData = Arc<RwLock<(BlockHeight, A)>>;
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
    U: UpdateAlgorithm<Algorithm = A> + Send,
    A: Send + Sync,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        let latest_block = self.latest_block_height();
        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                tracing::debug!("Stopping gas price service");
                should_continue = false;
            }
            new_algo = self.update_algorithm.next(latest_block) => {
                tracing::debug!("Updating gas price algorithm");
                self.update(next_block_height(&latest_block), new_algo)?;
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
