use crate::UpdateAlgorithm;
use fuel_core_types::fuel_types::BlockHeight;
pub use fuel_gas_price_algorithm::{
    AlgorithmUpdaterV1,
    AlgorithmV1,
    RecordedBlock,
};

#[cfg(test)]
mod tests;

pub struct FuelGasPriceUpdater<L2, Metadata> {
    inner: AlgorithmUpdaterV1,
    l2_block_source: L2,
    metadata_storage: Metadata,
}

impl<L2, Metadata> FuelGasPriceUpdater<L2, Metadata> {
    pub fn new(
        inner: AlgorithmUpdaterV1,
        l2_block_source: L2,
        metadata_storage: Metadata,
    ) -> Self {
        Self {
            inner,
            l2_block_source,
            metadata_storage,
        }
    }
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
    #[error("Failed to retrieve updater metadata at height {block_height:?}: {source_error:?}")]
    CouldNotFetchMetadata {
        block_height: BlockHeight,
        source_error: anyhow::Error,
    },
    #[error(
        "Failed to set updater metadata at height {block_height:?}: {source_error:?}"
    )]
    CouldNotSetMetadata {
        block_height: BlockHeight,
        source_error: anyhow::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

// Info required about the l2 block for the gas price algorithm
#[derive(Debug, Clone, PartialEq)]
pub struct BlockInfo {
    // Block height
    pub height: u32,
    // Fullness of block gas usage vs max block gas
    pub fullness: (u64, u64),
    // Block size in bytes
    pub block_bytes: u64,
    // Gas price of the block
    pub gas_price: u64,
}
#[async_trait::async_trait]
pub trait L2BlockSource: Send + Sync {
    async fn get_l2_block(&mut self, height: BlockHeight) -> Result<BlockInfo>;
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum UpdaterMetadata {
    V1(AlgorithmUpdaterV1),
}

impl UpdaterMetadata {
    pub fn l2_block_height(&self) -> BlockHeight {
        match self {
            UpdaterMetadata::V1(v1) => v1.l2_block_height.into(),
        }
    }
}

impl From<UpdaterMetadata> for AlgorithmUpdaterV1 {
    fn from(metadata: UpdaterMetadata) -> Self {
        match metadata {
            UpdaterMetadata::V1(v1) => v1,
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
    async fn get_metadata(
        &self,
        block_height: &BlockHeight,
    ) -> Result<Option<UpdaterMetadata>>;
    async fn set_metadata(&mut self, metadata: UpdaterMetadata) -> Result<()>;
}

impl<L2, Metadata> FuelGasPriceUpdater<L2, Metadata>
where
    Metadata: MetadataStorage,
{
    pub async fn init(
        init_metadata: UpdaterMetadata,
        l2_block_source: L2,
        metadata_storage: Metadata,
    ) -> Result<Self> {
        let target_block_height = init_metadata.l2_block_height();
        let inner = metadata_storage
            .get_metadata(&target_block_height)
            .await?
            .unwrap_or(init_metadata)
            .into();
        let updater = Self {
            inner,
            l2_block_source,
            metadata_storage,
        };
        Ok(updater)
    }
}

#[async_trait::async_trait]
impl<L2, Metadata> UpdateAlgorithm for FuelGasPriceUpdater<L2, Metadata>
where
    L2: L2BlockSource,
    Metadata: MetadataStorage + Send + Sync,
{
    type Algorithm = AlgorithmV1;

    fn start(&self, _for_block: BlockHeight) -> Self::Algorithm {
        self.inner.algorithm()
    }

    async fn next(&mut self) -> anyhow::Result<Self::Algorithm> {
        tokio::select! {
            l2_block = self.l2_block_source.get_l2_block(self.inner.l2_block_height.into()) => {
                tracing::info!("Received L2 block: {:?}", l2_block);
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
                self.metadata_storage
                    .set_metadata(self.inner.clone().into())
                    .await?;
                Ok(self.inner.algorithm())
            }
        }
    }
}
