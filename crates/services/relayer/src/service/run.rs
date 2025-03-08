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

#[cfg(test)]
mod test;

pub trait RelayerData: EthRemote + EthLocal {
    /// Wait for the Ethereum layer to finish syncing.
    fn wait_if_eth_syncing(
        &self,
    ) -> impl core::future::Future<Output = anyhow::Result<()>> + Send;

    /// Download the logs from the DA layer and write them
    /// to the database.
    fn download_logs(
        &mut self,

        eth_sync_gap: &state::EthSyncGap,
    ) -> impl core::future::Future<Output = anyhow::Result<Option<u64>>> + Send;

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
    let mut state = state::build_eth(relayer).await?;

    // Check if we need to sync.
    if let Some(eth_sync_gap) = state.needs_to_sync_eth() {
        // Download events and write them to the database.
        let latest_written_height = relayer.download_logs(&eth_sync_gap).await?;
        // update the local state, only if we have written something.
        if let Some(latest_written_height) = latest_written_height {
            state.set_local(latest_written_height);
        }
    }

    // Update the synced state.
    relayer.update_synced(&state);

    Ok(())
}
