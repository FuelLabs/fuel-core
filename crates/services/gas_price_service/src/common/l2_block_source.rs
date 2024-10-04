#[cfg(test)]
mod tests;

use crate::common::{
    fuel_core_storage_adapter::{
        get_block_info,
        GasPriceSettings,
        GasPriceSettingsProvider,
    },
    utils::{
        BlockInfo,
        Error as GasPriceError,
        Result as GasPriceResult,
    },
};
use anyhow::anyhow;
use fuel_core_services::stream::BoxStream;
use fuel_core_types::{
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
};
use tokio_stream::StreamExt;

#[async_trait::async_trait]
pub trait L2BlockSource: Send + Sync {
    async fn get_l2_block(&mut self) -> GasPriceResult<BlockInfo>;
}

pub struct FuelL2BlockSource<Settings> {
    genesis_block_height: BlockHeight,
    gas_price_settings: Settings,
    committed_block_stream: BoxStream<SharedImportResult>,
}

impl<Settings> FuelL2BlockSource<Settings> {
    pub fn new(
        genesis_block_height: BlockHeight,
        gas_price_settings: Settings,
        committed_block_stream: BoxStream<SharedImportResult>,
    ) -> Self {
        Self {
            genesis_block_height,
            gas_price_settings,
            committed_block_stream,
        }
    }
}

#[async_trait::async_trait]
impl<Settings> L2BlockSource for FuelL2BlockSource<Settings>
where
    Settings: GasPriceSettingsProvider + Send + Sync,
{
    async fn get_l2_block(&mut self) -> GasPriceResult<BlockInfo> {
        let block = &self
            .committed_block_stream
            .next()
            .await
            .ok_or({
                GasPriceError::CouldNotFetchL2Block {
                    source_error: anyhow!("No committed block found"),
                }
            })?
            .sealed_block
            .entity;

        match block.header().height().cmp(&self.genesis_block_height) {
            std::cmp::Ordering::Less => Err(GasPriceError::CouldNotFetchL2Block {
                source_error: anyhow!("Block precedes expected genesis block height"),
            }),
            std::cmp::Ordering::Equal => Ok(BlockInfo::GenesisBlock),
            std::cmp::Ordering::Greater => {
                let param_version = block.header().consensus_parameters_version;

                let GasPriceSettings {
                    gas_price_factor,
                    block_gas_limit,
                } = self.gas_price_settings.settings(&param_version)?;
                get_block_info(block, gas_price_factor, block_gas_limit)
            }
        }
    }
}
