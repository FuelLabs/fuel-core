use crate::UpdateAlgorithm;
use fuel_core_types::fuel_types::BlockHeight;
use fuel_gas_price_algorithm::{
    AlgorithmUpdaterV1,
    AlgorithmV1,
    RecordedBlock,
};

#[cfg(test)]
mod tests;

pub struct FuelGasPriceUpdater<L2, DA, Metadata> {
    inner: AlgorithmUpdaterV1,
    l2_block_source: L2,
    da_record_source: DA,
    _metadata_storage: Metadata,
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

#[derive(Debug, Clone)]
pub enum UpdaterMetadata {
    V1(AlgorithmUpdaterV1),
}

impl From<UpdaterMetadata> for AlgorithmUpdaterV1 {
    fn from(metadata: UpdaterMetadata) -> Self {
        match metadata {
            UpdaterMetadata::V1(v1) => v1.into(),
        }
    }
}

impl From<AlgorithmUpdaterV1> for UpdaterMetadata {
    fn from(v1: AlgorithmUpdaterV1) -> Self {
        UpdaterMetadata::V1(v1)
    }
}

#[async_trait::async_trait]
pub trait MetadataStorage: Send + Sync {
    async fn get_metadata(&self) -> Result<Option<UpdaterMetadata>>;
    async fn set_metadata(&self, metadata: UpdaterMetadata) -> Result<()>;
}

impl<L2, DA, Metadata> FuelGasPriceUpdater<L2, DA, Metadata>
where
    Metadata: MetadataStorage,
{
    pub async fn init(
        init_metadata: UpdaterMetadata,
        l2_block_source: L2,
        da_record_source: DA,
        metadata_storage: Metadata,
    ) -> Result<Self> {
        let inner = metadata_storage
            .get_metadata()
            .await?
            .unwrap_or(init_metadata)
            .into();
        let updater = Self {
            inner,
            l2_block_source,
            da_record_source,
            _metadata_storage: metadata_storage,
        };
        Ok(updater)
    }
}

#[async_trait::async_trait]
impl<L2: L2BlockSource, DA: DARecordSource, Metadata: Send + Sync> UpdateAlgorithm
    for FuelGasPriceUpdater<L2, DA, Metadata>
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
