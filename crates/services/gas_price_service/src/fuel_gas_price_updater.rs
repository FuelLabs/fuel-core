use crate::UpdateAlgorithm;
use fuel_core_types::fuel_types::BlockHeight;
use gas_price_algorithm::{
    AlgorithmUpdaterV1,
    AlgorithmV1,
    RecordedBlock,
};

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;
    use std::sync::Arc;
    use tokio::sync::{
        mpsc::Receiver,
        Mutex,
    };

    struct FakeL2BlockSource {
        l2_block: Arc<Mutex<Receiver<BlockInfo>>>,
    }

    #[async_trait::async_trait]
    impl L2BlockSource for FakeL2BlockSource {
        async fn get_l2_block(&self, _height: BlockHeight) -> Result<BlockInfo> {
            let block = self.l2_block.lock().await.recv().await.unwrap();
            Ok(block)
        }
    }

    struct FakeDARecordSource;

    #[async_trait::async_trait]
    impl DARecordSource for FakeDARecordSource {
        async fn get_da_record(&self) -> Result<Vec<RecordedBlock>> {
            Ok(vec![])
        }
    }

    fn arb_inner_updater() -> AlgorithmUpdaterV1 {
        AlgorithmUpdaterV1 {
            exec_gas_price_increase_amount: 10,
            l2_block_height: 0,
            l2_block_fullness_threshold_percent: 0,
            max_da_gas_price_change_percent: 0,
            total_da_rewards: 0,
            da_recorded_block_height: 0,
            latest_known_total_da_cost: 0,
            projected_total_da_cost: 0,
            da_p_component: 0,
            da_d_component: 0,
            profit_avg: 0,
            avg_window: 0,
            latest_da_cost_per_byte: 0,
            new_exec_price: 0,
            last_da_price: 0,
            unrecorded_blocks: vec![],
        }
    }

    #[tokio::test]
    async fn next__fetches_l2_block() {
        // given
        let l2_block = BlockInfo {
            height: 1,
            fullness: (50, 100),
            block_bytes: 1000,
            gas_price: 100,
        };
        let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
        let l2_block_source = FakeL2BlockSource {
            l2_block: Arc::new(Mutex::new(l2_block_receiver)),
        };

        let da_record_source = FakeDARecordSource;
        let inner = arb_inner_updater();
        let mut updater =
            FuelGasPriceUpdater::new(inner, l2_block_source, da_record_source);

        let start = updater.start(0.into());
        // when
        let next = tokio::spawn(async move { updater.next().await });

        l2_block_sender.send(l2_block).await.unwrap();
        let new = next.await.unwrap().unwrap();

        // then
        assert_ne!(start, new);
    }
}

pub struct FuelGasPriceUpdater<L2, DA> {
    inner: AlgorithmUpdaterV1,
    l2_block_source: L2,
    da_record_source: DA,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to find L2 block at height {block_height:?}: {source_error:?}")]
    CouldNotFetchL2Block {
        block_height: BlockHeight,
        source_error: anyhow::Error,
    },
    #[error("Failed to find DA records: {0:?}")]
    CouldNotFetchDARecord(anyhow::Error),
}

type Result<T> = std::result::Result<T, Error>;

pub struct BlockInfo {
    pub height: u32,
    pub fullness: (u64, u64),
    pub block_bytes: u64,
    pub gas_price: u64,
}
#[async_trait::async_trait]
pub trait L2BlockSource: Send + Sync {
    async fn get_l2_block(&self, height: BlockHeight) -> Result<BlockInfo>;
}

#[async_trait::async_trait]
pub trait DARecordSource: Send + Sync {
    async fn get_da_record(&self) -> Result<Vec<RecordedBlock>>;
}

impl<L2, DA> FuelGasPriceUpdater<L2, DA> {
    pub fn new(
        inner: AlgorithmUpdaterV1,
        l2_block_source: L2,
        da_record_source: DA,
    ) -> Self {
        Self {
            inner,
            l2_block_source,
            da_record_source,
        }
    }
}

#[async_trait::async_trait]
impl<L2: L2BlockSource, DA: DARecordSource> UpdateAlgorithm
    for FuelGasPriceUpdater<L2, DA>
{
    type Algorithm = AlgorithmV1;

    fn start(&self, _for_block: BlockHeight) -> Self::Algorithm {
        self.inner.algorithm()
    }

    async fn next(&mut self) -> anyhow::Result<Self::Algorithm> {
        tokio::select! {
            l2_block = self.l2_block_source.get_l2_block(self.inner.l2_block_height.into()) => {
                let l2_block = l2_block?;
                let BlockInfo {
                    height,
                    fullness,
                    block_bytes,
                    gas_price,
                } = l2_block;
                self.inner.update_l2_block_data(
                    height,
                    fullness,
                    block_bytes,
                    gas_price,
                )?;
                Ok(self.inner.algorithm())
            }
            da_record = self.da_record_source.get_da_record() => {
                let da_record = da_record?;
                self.inner.update_da_record_data(da_record)?;
                Ok(self.inner.algorithm())
            }
        }
    }
}
