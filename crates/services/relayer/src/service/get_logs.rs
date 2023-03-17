use super::*;
use futures::TryStreamExt;

#[cfg(test)]
mod test;

/// Download the logs from the DA layer.
pub(crate) fn download_logs<'a, P>(
    eth_sync_gap: &state::EthSyncGap,
    contracts: Vec<H160>,
    eth_node: &'a P,
    page_size: u64,
) -> impl futures::Stream<Item = Result<(u64, Vec<Log>), ProviderError>> + 'a
where
    P: Middleware<Error = ProviderError> + 'static,
{
    // Create a stream of paginated logs.
    futures::stream::try_unfold(
        eth_sync_gap.page(page_size),
        move |page: Option<state::EthSyncPage>| {
            let contracts = contracts.clone();
            async move {
                match page {
                    None => Ok(None),
                    Some(page) => {
                        // Create the log filter from the page.
                        let filter = Filter::new()
                            .from_block(page.oldest())
                            .to_block(page.latest())
                            .address(ValueOrArray::Array(contracts))
                            .topic0(*crate::config::ETH_LOG_MESSAGE);

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
pub(crate) async fn write_logs<D, S>(database: &mut D, logs: S) -> anyhow::Result<()>
where
    D: RelayerDb,
    S: futures::Stream<Item = Result<(u64, Vec<Log>), ProviderError>>,
{
    tokio::pin!(logs);
    while let Some((height, events)) = logs.try_next().await? {
        let messages = events
            .into_iter()
            .filter_map(|event| match EthEventLog::try_from(&event) {
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
        database.insert_messages(&height.into(), &messages)?;
    }
    Ok(())
}
