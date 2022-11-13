use super::*;
use futures::TryStreamExt;

#[cfg(test)]
mod test;

/// Download the logs from the DA layer.
pub(crate) fn download_logs<P>(
    eth_sync_gap: &state::EthSyncGap,
    contracts: Vec<H160>,
    eth_node: Arc<P>,
    page_size: u64,
) -> impl futures::Stream<Item = Result<(u64, Vec<Log>), ProviderError>>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    // Create a stream of paginated logs.
    futures::stream::try_unfold(
        eth_sync_gap.page(page_size),
        move |page: Option<state::EthSyncPage>| {
            let contracts = contracts.clone();
            let eth_node = eth_node.clone();
            async move {
                match page {
                    None => Ok(None),
                    Some(page) => {
                        // Create the log filter from the page.
                        let filter = Filter::new()
                            .from_block(page.oldest())
                            .to_block(page.latest())
                            .address(ValueOrArray::Array(contracts));

                        tracing::info!(
                            "Downloading logs for block range: {}..={}",
                            page.oldest(),
                            page.latest()
                        );

                        let latest_block = page.latest();

                        // Reduce the page.
                        let page = page.reduce();

                        // Get the logs and return the reduced page.
                        eth_node
                            .get_logs(&filter)
                            .await
                            .map(|logs| Some(((latest_block, logs), page)))
                    }
                }
            }
        },
    )
}

/// Write the logs to the database.
pub(crate) async fn write_logs<S>(
    database: &mut dyn RelayerDb,
    logs: S,
) -> anyhow::Result<()>
where
    S: futures::Stream<Item = Result<(u64, Vec<Log>), ProviderError>>,
{
    tokio::pin!(logs);
    while let Some((to_block, events)) = logs.try_next().await? {
        for event in events {
            let event: EthEventLog = (&event).try_into()?;
            match event {
                EthEventLog::Message(m) => {
                    use fuel_core_interfaces::common::fuel_storage::StorageMutate;
                    let m: Message = (&m).into();
                    // Add messages to database
                    StorageMutate::<Messages>::insert(database, &m.id(), &m)?;
                }
                // TODO: Log out ignored messages.
                EthEventLog::Ignored => (),
            }
        }
        database.set_finalized_da_height(to_block.into()).await;
    }
    Ok(())
}
