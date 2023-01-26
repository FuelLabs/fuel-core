//! The module provides the functionality that verifies the blocks and headers based
//! on the used consensus.

pub mod config;

#[cfg(test)]
mod tests;

use crate::block_verifier::config::Config;
use anyhow::ensure;
use fuel_core_poa::ports::Database as PoAVerifierDatabase;
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Consensus,
        header::BlockHeader,
        primitives::BlockHeight,
        SealedBlockHeader,
    },
    fuel_types::Bytes32,
    tai64::Tai64,
};

/// Verifier is responsible for validation of the blocks and headers.
pub struct Verifier<D, R> {
    config: Config,
    database: D,
    _relayer: R,
}

impl<D, R> Verifier<D, R> {
    /// Creates a new instance of the verifier.
    pub fn new(config: Config, database: D, _relayer: R) -> Self {
        Self {
            config,
            database,
            _relayer,
        }
    }
}

impl<D, R> Verifier<D, R>
where
    D: PoAVerifierDatabase,
{
    /// Verifies **all** fields of the block based on used consensus to produce a block.
    ///
    /// Return an error if the verification failed, otherwise `Ok(())`.
    pub fn verify_block_fields(
        &self,
        consensus: &Consensus,
        block: &Block,
    ) -> anyhow::Result<()> {
        match consensus {
            Consensus::Genesis(_) => {
                let expected_genesis_height = self
                    .config
                    .chain_config
                    .initial_state
                    .as_ref()
                    .map(|config| config.height.unwrap_or_else(|| 0u32.into()))
                    .unwrap_or_else(|| 0u32.into());
                verify_genesis_block_fields(expected_genesis_height, block.header())
            }
            Consensus::PoA(_) => fuel_core_poa::verifier::verify_block_fields(
                &self.config.poa,
                &self.database,
                block,
            ),
        }
    }

    /// Verifies the consensus of the block header.
    pub fn verify_consensus(&self, header: &SealedBlockHeader) -> bool {
        let SealedBlockHeader {
            entity: header,
            consensus,
        } = header;
        match consensus {
            Consensus::Genesis(_) => true,
            Consensus::PoA(consensus) => fuel_core_poa::verifier::verify_consensus(
                &self.config.chain_config.consensus,
                header,
                consensus,
            ),
        }
    }
}

fn verify_genesis_block_fields(
    expected_genesis_height: BlockHeight,
    header: &BlockHeader,
) -> anyhow::Result<()> {
    let actual_genesis_height = *header.height();

    ensure!(
        header.prev_root() == &Bytes32::zeroed(),
        "The genesis previous root should be zeroed"
    );
    ensure!(
        header.time() == Tai64::UNIX_EPOCH,
        "The genesis time should be unix epoch time"
    );
    ensure!(
        // TODO: Set `da_height` based on the chain config.
        header.da_height == Default::default(),
        "The genesis `da_height` is not as expected"
    );
    ensure!(
        expected_genesis_height == actual_genesis_height,
        "The genesis height is not as expected"
    );
    Ok(())
}
