use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::block::Block,
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};

use crate::{
    common::{
        updater_metadata::UpdaterMetadata,
        utils::Result,
    },
    v0::metadata::V0AlgorithmConfig,
    v1::metadata::V1AlgorithmConfig,
};

pub trait L2Data: Send + Sync {
    fn latest_height(&self) -> StorageResult<BlockHeight>;
    fn get_block(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<Block<Transaction>>>;
}

pub trait SetMetadataStorage: Send + Sync {
    fn set_metadata(&mut self, metadata: &UpdaterMetadata) -> Result<()>;
}

pub trait GetMetadataStorage: Send + Sync {
    fn get_metadata(&self, block_height: &BlockHeight)
        -> Result<Option<UpdaterMetadata>>;
}

pub trait SetDaSequenceNumber: Send + Sync {
    fn set_sequence_number(
        &mut self,
        block_height: &BlockHeight,
        sequence_number: u32,
    ) -> Result<()>;
}

pub trait GetDaSequenceNumber: Send + Sync {
    fn get_sequence_number(&self, block_height: &BlockHeight) -> Result<Option<u32>>;
}

pub trait TransactionableStorage: Send + Sync {
    type Transaction<'a>
    where
        Self: 'a;

    fn begin_transaction(&mut self) -> Result<Self::Transaction<'_>>;
    fn commit_transaction(transaction: Self::Transaction<'_>) -> Result<()>;
}

/// Provides the latest block height.
/// This is used to determine the latest block height that has been processed by the gas price service.
/// We need this to fetch the gas price data for the latest block.
pub trait GasPriceData: Send + Sync {
    fn latest_height(&self) -> Option<BlockHeight>;
}

pub enum GasPriceServiceConfig {
    V0(V0AlgorithmConfig),
    V1(V1AlgorithmConfig),
}

impl GasPriceServiceConfig {
    pub fn new_v0(
        starting_gas_price: u64,
        min_gas_price: u64,
        gas_price_change_percent: u64,
        gas_price_threshold_percent: u64,
    ) -> Self {
        Self::V0(V0AlgorithmConfig {
            starting_gas_price,
            min_gas_price,
            gas_price_change_percent,
            gas_price_threshold_percent,
        })
    }

    pub fn new_v1(metadata: V1AlgorithmConfig) -> Self {
        Self::V1(metadata)
    }

    /// Extract V0AlgorithmConfig if it is of V0 version
    pub fn v0(self) -> Option<V0AlgorithmConfig> {
        if let GasPriceServiceConfig::V0(v0) = self {
            Some(v0)
        } else {
            None
        }
    }

    /// Extract V1AlgorithmConfig if it is of V1 version
    pub fn v1(self) -> Option<V1AlgorithmConfig> {
        if let GasPriceServiceConfig::V1(v1) = self {
            Some(v1)
        } else {
            None
        }
    }
}

impl From<V0AlgorithmConfig> for GasPriceServiceConfig {
    fn from(value: V0AlgorithmConfig) -> Self {
        GasPriceServiceConfig::V0(value)
    }
}

impl From<V1AlgorithmConfig> for GasPriceServiceConfig {
    fn from(value: V1AlgorithmConfig) -> Self {
        GasPriceServiceConfig::V1(value)
    }
}
