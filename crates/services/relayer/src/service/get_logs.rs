use super::*;
use fuel_core_types::services::relayer::Event;
use futures::TryStreamExt;
use std::collections::BTreeMap;

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
    while let Some((last_height, events)) = logs.try_next().await? {
        let last_height = last_height.into();
        let mut ordered_events = BTreeMap::<DaBlockHeight, Vec<Event>>::new();
        let fuel_events =
            events
                .into_iter()
                .filter_map(|event| match EthEventLog::try_from(&event) {
                    Ok(event) => {
                        match event {
                            EthEventLog::Message(m) => {
                                Some(Ok(Event::Message(Message::from(&m))))
                            }
                            // TODO: Log out ignored messages.
                            EthEventLog::Ignored => None,
                        }
                    }
                    Err(e) => Some(Err(e)),
                });

        for event in fuel_events {
            let event = event?;
            let height = event.da_height();
            ordered_events.entry(height).or_default().push(event);
        }

        let mut inserted_last_height = false;
        for (height, events) in ordered_events {
            database.insert_events(&height, &events)?;
            if height == last_height {
                inserted_last_height = true;
            }
        }

        if !inserted_last_height {
            database.insert_events(&last_height, &[])?;
        }
    }
    Ok(())
}
