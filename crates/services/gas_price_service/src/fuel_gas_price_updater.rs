use crate::{
    GasPriceAlgorithm,
    UpdateAlgorithm,
};
use anyhow::anyhow;
use core::num::NonZeroU64;
use fuel_core_types::fuel_types::BlockHeight;
pub use fuel_gas_price_algorithm::{
    v0::{
        AlgorithmUpdaterV0,
        AlgorithmV0,
    },
    v1::{
        AlgorithmUpdaterV1,
        AlgorithmV1,
        RecordedBlock,
    },
};

#[cfg(test)]
mod tests;

pub mod fuel_core_storage_adapter;

pub mod da_source_adapter;

pub struct FuelGasPriceUpdater<Metadata> {
    inner: AlgorithmUpdater,
    metadata_storage: Metadata,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AlgorithmUpdater {
    V0(AlgorithmUpdaterV0),
    V1(AlgorithmUpdaterV1),
}

impl AlgorithmUpdater {
    pub fn algorithm(&self) -> Algorithm {
        match self {
            AlgorithmUpdater::V0(v0) => Algorithm::V0(v0.algorithm()),
            AlgorithmUpdater::V1(v1) => Algorithm::V1(v1.algorithm()),
        }
    }

    pub fn l2_block_height(&self) -> BlockHeight {
        match self {
            AlgorithmUpdater::V0(v0) => v0.l2_block_height.into(),
            AlgorithmUpdater::V1(v1) => v1.l2_block_height.into(),
        }
    }
}

impl<Metadata> FuelGasPriceUpdater<Metadata> {
    pub fn new(inner: AlgorithmUpdater, metadata_storage: Metadata) -> Self {
        Self {
            inner,
            metadata_storage,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to find L2 block: {source_error:?}")]
    CouldNotFetchL2Block { source_error: anyhow::Error },
    #[error("Failed to compute: {source_error:?}")]
    MathError { source_error: anyhow::Error },
    #[error("No Mint transaction in block")]
    NoMintTx,
    #[error("Failed to find DA records: {0:?}")]
    CouldNotFetchDARecord(anyhow::Error),
    #[error("Failed to retrieve updater metadata: {source_error:?}")]
    CouldNotFetchMetadata { source_error: anyhow::Error },
    #[error(
        "Failed to set updater metadata at height {block_height:?}: {source_error:?}"
    )]
    CouldNotSetMetadata {
        block_height: BlockHeight,
        source_error: anyhow::Error,
    },
    #[error("Failed to initialize updater: {0:?}")]
    CouldNotInitUpdater(anyhow::Error),
}

pub type Result<T, E = Error> = core::result::Result<T, E>;

// Info required about the l2 block for the gas price algorithm
#[derive(Debug, Clone, PartialEq)]
pub enum BlockInfo {
    // The genesis block of the L2 chain
    GenesisBlock,
    // A normal block in the L2 chain
    Block {
        // Block height
        height: u32,
        // Gas used in the block
        gas_used: u64,
        // Total gas capacity of the block
        block_gas_capacity: u64,
    },
}
#[async_trait::async_trait]
pub trait L2BlockSource: Send + Sync {
    async fn get_l2_block(&mut self) -> Result<BlockInfo>;
}

#[derive(Debug, Default, Clone, Eq, Hash, PartialEq)]
pub struct DaBlockCosts {
    pub l2_block_range: core::ops::Range<u32>,
    pub blob_size_bytes: u32,
    pub blob_cost_wei: u128,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum UpdaterMetadata {
    V0(V0Metadata),
}

impl UpdaterMetadata {
    pub fn l2_block_height(&self) -> BlockHeight {
        match self {
            UpdaterMetadata::V0(v1) => v1.l2_block_height.into(),
        }
    }
}

impl From<UpdaterMetadata> for AlgorithmUpdater {
    fn from(metadata: UpdaterMetadata) -> Self {
        match metadata {
            UpdaterMetadata::V0(v1_no_da) => {
                let V0Metadata {
                    new_exec_price,
                    min_exec_gas_price,
                    exec_gas_price_change_percent,
                    l2_block_height,
                    l2_block_fullness_threshold_percent,
                } = v1_no_da;
                let updater = AlgorithmUpdaterV0 {
                    new_exec_price,
                    min_exec_gas_price,
                    exec_gas_price_change_percent,
                    l2_block_height,
                    l2_block_fullness_threshold_percent,
                };
                AlgorithmUpdater::V0(updater)
            }
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct V0Metadata {
    /// The gas price to cover the execution of the next block
    pub new_exec_price: u64,
    // Execution
    /// The lowest the algorithm allows the exec gas price to go
    pub min_exec_gas_price: u64,
    /// The Percentage the execution gas price will change in a single block, either increase or decrease
    /// based on the fullness of the last L2 block
    pub exec_gas_price_change_percent: u64,
    /// The height for which the `new_exec_price` is calculated, which should be the _next_ block
    pub l2_block_height: u32,
    /// The threshold of gas usage above and below which the gas price will increase or decrease
    /// This is a percentage of the total capacity of the L2 block
    pub l2_block_fullness_threshold_percent: u64,
}

impl From<AlgorithmUpdater> for UpdaterMetadata {
    fn from(updater: AlgorithmUpdater) -> Self {
        match updater {
            AlgorithmUpdater::V0(v0) => {
                let metadata = V0Metadata {
                    new_exec_price: v0.new_exec_price,
                    min_exec_gas_price: v0.min_exec_gas_price,
                    exec_gas_price_change_percent: v0.exec_gas_price_change_percent,
                    l2_block_height: v0.l2_block_height,
                    l2_block_fullness_threshold_percent: v0
                        .l2_block_fullness_threshold_percent,
                };
                UpdaterMetadata::V0(metadata)
            }
            AlgorithmUpdater::V1(_v1) => {
                unimplemented!() // https://github.com/FuelLabs/fuel-core/issues/2140
            }
        }
    }
}

pub trait MetadataStorage: Send + Sync {
    fn get_metadata(&self, block_height: &BlockHeight)
        -> Result<Option<UpdaterMetadata>>;
    fn set_metadata(&mut self, metadata: UpdaterMetadata) -> Result<()>;
}

impl<Metadata> FuelGasPriceUpdater<Metadata>
where
    Metadata: MetadataStorage,
{
    pub fn init(
        target_block_height: BlockHeight,
        metadata_storage: Metadata,
        min_exec_gas_price: u64,
        exec_gas_price_change_percent: u64,
        l2_block_fullness_threshold_percent: u64,
    ) -> Result<Self> {
        let old_metadata = metadata_storage.get_metadata(&target_block_height)?.ok_or(
            Error::CouldNotInitUpdater(anyhow::anyhow!(
                "No metadata found for block height: {:?}",
                target_block_height
            )),
        )?;
        let inner = match old_metadata {
            UpdaterMetadata::V0(old) => {
                let v0 = AlgorithmUpdaterV0::new(
                    old.new_exec_price,
                    min_exec_gas_price,
                    exec_gas_price_change_percent,
                    old.l2_block_height,
                    l2_block_fullness_threshold_percent,
                );
                AlgorithmUpdater::V0(v0)
            }
        };
        let updater = Self {
            inner,
            metadata_storage,
        };
        Ok(updater)
    }

    fn validate_block_gas_capacity(
        &self,
        block_gas_capacity: u64,
    ) -> anyhow::Result<NonZeroU64> {
        NonZeroU64::new(block_gas_capacity)
            .ok_or_else(|| anyhow!("Block gas capacity must be non-zero"))
    }

    async fn set_metadata(&mut self) -> anyhow::Result<()> {
        self.metadata_storage
            .set_metadata(self.inner.clone().into())
            .map_err(|err| anyhow!(err))
    }

    async fn handle_normal_block(
        &mut self,
        height: u32,
        gas_used: u64,
        block_gas_capacity: u64,
        _da_block_costs: Option<DaBlockCosts>,
    ) -> anyhow::Result<()> {
        let capacity = self.validate_block_gas_capacity(block_gas_capacity)?;

        match &mut self.inner {
            AlgorithmUpdater::V0(updater) => {
                updater.update_l2_block_data(height, gas_used, capacity)?;
            }
            AlgorithmUpdater::V1(_) => {
                return Err(anyhow!("V1 of the gas price algo has not been enabled yet"))
                // TODO(#2139): update the DA record data with data received from the source
                // updater.update_da_record_data(vec![])?;
            }
        }

        self.set_metadata().await?;
        Ok(())
    }

    async fn apply_block_info_to_gas_algorithm(
        &mut self,
        l2_block: BlockInfo,
        da_block_costs: Option<DaBlockCosts>,
    ) -> anyhow::Result<()> {
        match l2_block {
            BlockInfo::GenesisBlock => {
                self.set_metadata().await?;
            }
            BlockInfo::Block {
                height,
                gas_used,
                block_gas_capacity,
            } => {
                self.handle_normal_block(
                    height,
                    gas_used,
                    block_gas_capacity,
                    da_block_costs,
                )
                .await?;
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl<Metadata> UpdateAlgorithm for FuelGasPriceUpdater<Metadata>
where
    Metadata: MetadataStorage + Send + Sync,
{
    type Algorithm = Algorithm;

    fn start(&self, _for_block: BlockHeight) -> Self::Algorithm {
        self.inner.algorithm()
    }

    async fn next(
        &mut self,
        l2_block: BlockInfo,
        da_block_costs: Option<DaBlockCosts>,
    ) -> anyhow::Result<Self::Algorithm> {
        self.apply_block_info_to_gas_algorithm(l2_block, da_block_costs)
            .await?;

        Ok(self.inner.algorithm())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Algorithm {
    V0(AlgorithmV0),
    V1(AlgorithmV1),
}

impl GasPriceAlgorithm for Algorithm {
    fn next_gas_price(&self) -> u64 {
        match self {
            Algorithm::V0(v0) => v0.calculate(),
            Algorithm::V1(v1) => v1.calculate(0),
        }
    }

    fn worst_case_gas_price(&self, height: BlockHeight) -> u64 {
        match self {
            Algorithm::V0(v0) => v0.worst_case(height.into()),
            Algorithm::V1(v1) => v1.calculate(0),
        }
    }
}
