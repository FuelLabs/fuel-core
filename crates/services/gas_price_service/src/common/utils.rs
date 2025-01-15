use fuel_core_types::fuel_types::BlockHeight;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to find L2 block: {source_error:?}")]
    CouldNotFetchL2Block { source_error: anyhow::Error },
    #[error("Failed to get the recorded height: {0:?}")]
    CouldNotFetchRecordedHeight(anyhow::Error),
    #[error("Failed to set the recorded height: {0:?}")]
    CouldNotSetRecordedHeight(anyhow::Error),
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
    #[error("Failed to convert metadata to concrete type. There is no migration path for this metadata version")]
    CouldNotConvertMetadata, // todo(https://github.com/FuelLabs/fuel-core/issues/2286)
    #[error("Failed to commit to storage: {0:?}")]
    CouldNotCommit(anyhow::Error),
}

pub type Result<T, E = Error> = core::result::Result<T, E>;

// Info required about the l2 block for the gas price algorithm
#[derive(Debug, Clone, Copy, PartialEq)]
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
        // The size of block in bytes
        block_bytes: u64,
        // The fees the block has collected
        block_fees: u64,
        // The gas price used in the block
        gas_price: u64,
    },
}
