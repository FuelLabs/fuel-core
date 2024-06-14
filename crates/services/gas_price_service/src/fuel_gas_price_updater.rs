use crate::UpdateAlgorithm;
use fuel_core_types::fuel_types::BlockHeight;
use gas_price_algorithm::{
    AlgorithmUpdaterV1,
    AlgorithmV1,
    RecordedBlock,
};

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
