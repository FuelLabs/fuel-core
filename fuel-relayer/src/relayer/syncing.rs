use super::*;

pub async fn wait_if_eth_syncing<P>(eth_node: &P) -> anyhow::Result<()>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    let mut count = 0;
    while get_status(eth_node).await? {
        count += 1;
        if count > 12 {
            tracing::warn!(
                "relayer has been waiting longer then a minute for the eth node to sync"
            )
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    Ok(())
}

async fn get_status<P>(eth_node: &P) -> anyhow::Result<bool>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    let status = match eth_node.syncing().await {
        Ok(s) => s,
        Err(err) => anyhow::bail!("Failed to check if DA layer is syncing {}", err),
    };
    Ok(matches!(status, SyncingStatus::IsSyncing { .. }))
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::middleware::{
        MockMiddleware,
        TriggerType,
    };

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn handles_syncing() {
        let eth_node = MockMiddleware::default();
        eth_node.update_data(|data| {
            data.is_syncing = SyncingStatus::IsSyncing {
                starting_block: 0.into(),
                current_block: 0.into(),
                highest_block: 0.into(),
            }
        });

        let mut count = 0;
        eth_node.set_before_event(move |data, evt| {
            if let TriggerType::Syncing = evt {
                count += 1;
                if count == 2 {
                    data.is_syncing = SyncingStatus::IsFalse;
                }
            }
        });

        let before = tokio::time::Instant::now();
        wait_if_eth_syncing(&eth_node).await.unwrap();
        let after = tokio::time::Instant::now();

        assert_eq!(
            before.elapsed().checked_sub(after.elapsed()),
            Some(Duration::from_secs(5))
        );
    }
}
