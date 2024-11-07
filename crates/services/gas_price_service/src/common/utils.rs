use fuel_core_types::fuel_types::BlockHeight;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to find L2 block: {source_error:?}")]
    CouldNotFetchL2Block { source_error: anyhow::Error },
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
    #[error("Failed to convert metadata to concrete type. THere is no migration path for this metadata version")]
    CouldNotConvertMetadata, // todo(https://github.com/FuelLabs/fuel-core/issues/2286)
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
