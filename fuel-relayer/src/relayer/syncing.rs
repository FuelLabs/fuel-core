use ethers_core::types::U256;

use super::*;

#[cfg(test)]
mod tests;

struct Status {
    starting_block: U256,
    current_block: U256,
    highest_block: U256,
}

pub async fn wait_if_eth_syncing<P>(
    eth_node: &P,
    sync_call_freq: Duration,
    sync_log_freq: Duration,
) -> anyhow::Result<()>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    let mut start = tokio::time::Instant::now();
    let mut loop_time = tokio::time::Instant::now();
    while let SyncingStatus::IsSyncing(is_syncing) = get_status(eth_node).await? {
        if start.elapsed() > sync_log_freq {
            let status = Status {
                starting_block: is_syncing.starting_block.as_u64().into(),
                current_block: is_syncing.current_block.as_u64().into(),
                highest_block: is_syncing.highest_block.as_u64().into(),
            };
            start = tokio::time::Instant::now();
            tracing::info!(
                "Waiting for the Ethereum endpoint to finish syncing. \nStatus {}",
                status
            );
        }
        tokio::time::sleep(sync_call_freq.saturating_sub(loop_time.elapsed())).await;
        loop_time = tokio::time::Instant::now();
    }
    Ok(())
}

async fn get_status<P>(eth_node: &P) -> anyhow::Result<SyncingStatus>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    match eth_node.syncing().await {
        Ok(s) => Ok(s),
        Err(err) => Err(anyhow::anyhow!(
            "Failed to check if DA layer is syncing {}",
            err
        )),
    }
}

impl core::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size: u32 = self
            .highest_block
            .saturating_sub(self.starting_block)
            .try_into()
            .unwrap_or_default();
        let progress: u32 = self
            .current_block
            .saturating_sub(self.starting_block)
            .try_into()
            .unwrap_or_default();
        let complete = if progress == size || size == 0 {
            100.0
        } else if progress == 0 {
            0.0
        } else {
            (f64::from(progress) / f64::from(size)) * 100.0
        };

        write!(
            f,
            "from {} to {} currently at {}. {:.0}% Done.",
            self.starting_block, self.highest_block, self.current_block, complete
        )
    }
}
