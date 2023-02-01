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
) -> impl futures::Stream<Item = Result<Vec<Log>, ProviderError>>
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

                        // Reduce the page.
                        let page = page.reduce();

                        // Get the logs and return the reduced page.
                        eth_node
                            .get_logs(&filter)
                            .await
                            .map(|logs| Some((logs, page)))
                    }
                }
            }
        },
    )
}

/// Write the logs to the database.
pub(crate) async fn write_logs<D, S>(database: &mut D, logs: S) -> anyhow::Result<()>
where
    D: RelayerDb,
    S: futures::Stream<Item = Result<Vec<Log>, ProviderError>>,
{
    tokio::pin!(logs);
    while let Some(events) = logs.try_next().await? {
        let messages = events
            .into_iter()
            .map(|event| {
                let event: EthEventLog = (&event).try_into()?;
                Ok(event)
            })
            .filter_map(|result| match result {
                Ok(event) => {
                    match event {
                        EthEventLog::Message(m) => Some(Ok(Message::from(&m).check())),
                        // TODO: Log out ignored messages.
                        EthEventLog::Ignored => None,
                    }
                }
                Err(e) => Some(Err(e)),
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        database.insert_messages(&messages)?;
    }
    Ok(())
}
