use fuel_core_services::seqlock::SeqLock;
use fuel_core_types::fuel_types::BlockHeight;
use std::sync::Arc;

pub trait GasPriceAlgorithm: Copy {
    fn next_gas_price(&self) -> u64;
    fn worst_case_gas_price(&self, block_height: BlockHeight) -> u64;
}

#[derive(Debug, Default)]
pub struct SharedGasPriceAlgo<A>(Arc<SeqLock<A>>);

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
        Self(Arc::new(SeqLock::new(algorithm)))
    }

    pub fn update(&mut self, new_algo: A) {
        self.0.write(|data| {
            *data = new_algo;
        });
    }
}

impl<A> SharedGasPriceAlgo<A>
where
    A: GasPriceAlgorithm + Send + Sync,
{
    pub fn next_gas_price(&self) -> u64 {
        self.0.read().next_gas_price()
    }

    pub fn worst_case_gas_price(&self, block_height: BlockHeight) -> u64 {
        self.0.read().worst_case_gas_price(block_height)
    }
}
