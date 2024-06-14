use crate::UpdateAlgorithm;
use fuel_core_types::fuel_types::BlockHeight;
use gas_price_algorithm::{
    AlgorithmUpdaterV1,
    AlgorithmV1,
};

pub struct FuelGasPriceUpdater<L2> {
    inner: AlgorithmUpdaterV1,
    l2_block_source: L2,
}

impl<L2> FuelGasPriceUpdater<L2> {
    pub fn new(inner: AlgorithmUpdaterV1, l2_block_source: L2) -> Self {
        Self {
            inner,
            l2_block_source,
        }
    }
}

impl<L2> UpdateAlgorithm for FuelGasPriceUpdater<L2> {
    type Algorithm = AlgorithmV1;

    fn start(&self, _for_block: BlockHeight) -> Self::Algorithm {
        self.inner.algorithm()
    }

    async fn next(&mut self) -> Self::Algorithm {
        todo!()
    }
}
