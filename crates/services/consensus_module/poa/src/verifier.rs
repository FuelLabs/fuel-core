use crate::ports::Database;
use anyhow::ensure;
use fuel_core_chain_config::ConsensusConfig;
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::poa::PoAConsensus,
        header::BlockHeader,
    },
    fuel_tx::Input,
};

#[cfg(test)]
mod tests;

// TODO: Make this function `async` and await the synchronization with the relayer.
pub fn verify_consensus(
    consensus_config: &ConsensusConfig,
    header: &BlockHeader,
    consensus: &PoAConsensus,
) -> bool {
    match consensus_config {
        ConsensusConfig::PoA { signing_key } => {
            let id = header.id();
            let m = id.as_message();
            consensus
                .signature
                .recover(m)
                .map_or(false, |k| Input::owner(&k) == *signing_key)
        }
        ConsensusConfig::PoAV2(poa) => {
            let id = header.id();
            let m = id.as_message();
            let signing_key = poa.signing_key_at(*header.height());
            consensus
                .signature
                .recover(m)
                .map_or(false, |k| Input::owner(&k) == signing_key)
        }
    }
}

pub fn verify_block_fields<D: Database>(
    database: &D,
    block: &Block,
) -> anyhow::Result<()> {
    let height = *block.header().height();
    ensure!(
        height != 0u32.into(),
        "The PoA block can't have the zero height"
    );

    let prev_height = height.pred().expect("We checked the height above");
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

    ensure!(
        header.time() >= prev_header.time(),
        "The `time` of the next block can't be lower"
    );

    ensure!(
        header.application_hash() == &header.application().hash(),
        "The application hash mismatch."
    );

    ensure!(
        header.validate_transactions(block.transactions()),
        "The transactions don't match header."
    );

    Ok(())
}
