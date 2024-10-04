use crate::{
    common::{
        updater_metadata::UpdaterMetadata,
        utils::Result,
    },
    v0::metadata::V0MetadataInitializer,
    v1::metadata::V1MetadataInitializer,
};
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::block::Block,
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};

pub trait L2Data: Send + Sync {
    fn latest_height(&self) -> StorageResult<BlockHeight>;
    fn get_block(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<Block<Transaction>>>;
}

pub trait MetadataStorage: Send + Sync {
    fn get_metadata(&self, block_height: &BlockHeight)
        -> Result<Option<UpdaterMetadata>>;
    fn set_metadata(&mut self, metadata: &UpdaterMetadata) -> Result<()>;
}

/// Provides the latest block height.
/// This is used to determine the latest block height that has been processed by the gas price service.
/// We need this to fetch the gas price data for the latest block.
pub trait GasPriceData: Send + Sync {
    fn latest_height(&self) -> Option<BlockHeight>;
}

pub enum VersionSpecificConfig {
    V0(V0MetadataInitializer),
    V1(V1MetadataInitializer),
}

impl VersionSpecificConfig {
    pub fn new_v0(
        starting_gas_price: u64,
        min_gas_price: u64,
        gas_price_change_percent: u64,
        gas_price_threshold_percent: u64,
    ) -> Self {
        Self::V0(V0MetadataInitializer {
            starting_gas_price,
            min_gas_price,
            gas_price_change_percent,
            gas_price_threshold_percent,
        })
    }

    pub fn new_v1(metadata: V1MetadataInitializer) -> Self {
        Self::V1(metadata)
    }

    /// Extract V0MetadataInitializer if it is of V0 version
    pub fn v0(self) -> Option<V0MetadataInitializer> {
        if let VersionSpecificConfig::V0(v0) = self {
            Some(v0)
        } else {
            None
        }
    }

    /// Extract V1MetadataInitializer if it is of V1 version
    pub fn v1(self) -> Option<V1MetadataInitializer> {
        if let VersionSpecificConfig::V1(v1) = self {
            Some(v1)
        } else {
            None
        }
    }
}

pub struct GasPriceServiceConfig {
    version_specific_config: VersionSpecificConfig,
}

impl GasPriceServiceConfig {
    pub fn new(version_specific_config: VersionSpecificConfig) -> Self {
        Self {
            version_specific_config,
        }
    }

    pub fn v0(self) -> Option<V0MetadataInitializer> {
        self.version_specific_config.v0()
    }

    pub fn v1(self) -> Option<V1MetadataInitializer> {
        self.version_specific_config.v1()
    }
}
