use super::{
    state,
    state::{
        EthLocal,
        EthRemote,
        EthState,
    },
};

use async_trait::async_trait;
use ethers_core::types::Log;
use ethers_providers::ProviderError;
use fuel_core_interfaces::model::DaBlockHeight;

#[cfg(test)]
mod test;

#[async_trait]
pub trait RelayerData: EthRemote + EthLocal {
    async fn wait_if_eth_syncing(&self) -> anyhow::Result<()>;

    async fn download_logs(
        &self,
        eth_sync_gap: &state::EthSyncGap,
    ) -> Result<Vec<Log>, ProviderError>;

    async fn write_logs(&mut self, logs: Vec<Log>) -> anyhow::Result<()>;

    async fn set_finalized_da_height(&self, height: DaBlockHeight);

    fn update_synced(&self, state: &EthState);
}

pub async fn run(relayer: &mut (dyn RelayerData + Send + Sync)) -> anyhow::Result<()> {
    // Await the eth node to sync.
    relayer.wait_if_eth_syncing().await?;

    let state = state::build_eth(relayer).await?;

    if let Some(eth_sync_gap) = state.needs_to_sync_eth() {
        // Download events
        let logs = relayer.download_logs(&eth_sync_gap).await?;

        // Turn logs into eth events
        relayer.write_logs(logs).await?;

        // Update finalized height in database.
        relayer
            .set_finalized_da_height(eth_sync_gap.latest().into())
            .await;
    }

    relayer.update_synced(&state);

    Ok(())
}
