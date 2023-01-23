use anyhow::ensure;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::{
        block::Block,
        header::BlockHeader,
        primitives::BlockHeight,
    },
    fuel_types::Bytes32,
};

#[cfg(test)]
mod tests;

/// The config of the block verifier.
pub struct Config {
    /// If the manual block is enabled, skip verification of some fields.
    pub enabled_manual_blocks: bool,
}

#[cfg_attr(test, mockall::automock)]
/// The port for the database.
pub trait Database {
    /// Gets the block header at `height`.
    fn block_header(&self, height: &BlockHeight) -> StorageResult<BlockHeader>;

    /// Gets the block header BMT MMR root at `height`.
    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32>;
}

pub fn verify_poa_block_fields<D: Database>(
    config: &Config,
    database: &D,
    block: &Block,
) -> anyhow::Result<()> {
    let height = *block.header().height();
    ensure!(
        height != 0u32.into(),
        "The PoA block can't have the zero height"
    );

    let prev_height = height - 1u32.into();
    let prev_root = database.block_header_merkle_root(&prev_height)?;
    let header = block.header();
    ensure!(
        header.prev_root() == &prev_root,
        "Previous root of the next block should match the previous block root"
    );

    let prev_header = database.block_header(&prev_height)?;

    ensure!(
        header.da_height >= prev_header.da_height,
        "The `da_height` of the next block can't be lower"
    );

    // Skip the verification of the time if it is possible to produce blocks manually.
    if !config.enabled_manual_blocks {
        ensure!(
            header.time() >= prev_header.time(),
            "The `time` of the next block can't be lower"
        );
    }

    ensure!(
        header.consensus.application_hash == header.application.hash(),
        "The application hash mismatch."
    );

    // TODO: We can check the root of the transactions and the root of the messages here.
    //  But we do the same in the executor right now during validation mode. I will not check
    //  it for now. But after merge of the https://github.com/FuelLabs/fuel-core/pull/889 it
    //  is should be easy to do with the `validate_transactions` method. And maybe we want
    //  to remove this check from the executor and replace it with check that transaction
    //  id is not modified during the execution.

    Ok(())
}
