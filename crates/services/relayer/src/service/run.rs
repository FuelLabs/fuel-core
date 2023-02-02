//! # run
//! This module handles the logic for the main run loop.

use super::{
    state,
    state::{
        EthLocal,
        EthRemote,
        EthState,
    },
};

use async_trait::async_trait;

#[cfg(test)]
mod test;

#[async_trait]
pub trait RelayerData: EthRemote + EthLocal {
    /// Wait for the Ethereum layer to finish syncing.
    async fn wait_if_eth_syncing(&self) -> anyhow::Result<()>;

    /// Download the logs from the DA layer and write them
    /// to the database.
    async fn download_logs(
        &mut self,
        eth_sync_gap: &state::EthSyncGap,
    ) -> anyhow::Result<()>;

    /// Update the synced state.
    fn update_synced(&self, state: &EthState);
}

/// A single iteration of the run loop.
pub async fn run<R>(relayer: &mut R) -> anyhow::Result<()>
where
    R: RelayerData,
{
    // Await the eth node to sync.
    relayer.wait_if_eth_syncing().await?;

    // Build the DA layer state.
    let state = state::build_eth(relayer).await?;

    // Check if we need to sync.
    if let Some(eth_sync_gap) = state.needs_to_sync_eth() {
        // Download events and write them to the database.
        relayer.download_logs(&eth_sync_gap).await?;
    }

    // Update the synced state.
    relayer.update_synced(&state);

    Ok(())
}
