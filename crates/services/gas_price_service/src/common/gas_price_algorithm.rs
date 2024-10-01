use fuel_core_types::fuel_types::BlockHeight;
use std::sync::Arc;
use tokio::sync::RwLock;

pub trait GasPriceAlgorithm {
    fn next_gas_price(&self) -> u64;
    fn worst_case_gas_price(&self, block_height: BlockHeight) -> u64;
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
