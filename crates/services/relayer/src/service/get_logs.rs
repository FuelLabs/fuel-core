use super::*;
use fuel_core_types::{
    entities::RelayedTransaction,
    fuel_types::Bytes20,
    services::relayer::Event,
};
use futures::TryStreamExt;
use std::collections::HashMap;

#[cfg(test)]
mod test;

pub struct DownloadedLogs {
    pub start_height: u64,
    pub last_height: u64,
    pub logs: Vec<Log>,
}

/// Download the logs from the DA layer.
pub(crate) fn download_logs<'a, P>(
    eth_sync_gap: &state::EthSyncGap,
    contracts: Vec<Bytes20>,
    eth_node: &'a P,
    page_size: u64,
) -> impl futures::Stream<Item = Result<DownloadedLogs, ProviderError>> + 'a
where
    P: Middleware<Error = ProviderError> + 'static,
{
    // Create a stream of paginated logs.
    futures::stream::try_unfold(
        eth_sync_gap.page(page_size),
        move |page: Option<state::EthSyncPage>| {
            let contracts = contracts
                .iter()
                .map(|c| ethereum_types::Address::from_slice(c.as_slice()))
                .collect();
            async move {
                match page {
                    None => Ok(None),
                    Some(page) => {
                        // Create the log filter from the page.
                        let filter = Filter::new()
                            .from_block(page.oldest())
                            .to_block(page.latest())
                            .address(ValueOrArray::Array(contracts))
                            .topic0(ValueOrArray::Array(vec![
                                *crate::config::ETH_LOG_MESSAGE,
                                *crate::config::ETH_FORCED_TX,
                            ]));

                        tracing::info!(
                            "Downloading logs for block range: {}..={}",
                            page.oldest(),
                            page.latest()
                        );

                        let oldest_block = page.oldest();
                        let latest_block = page.latest();

                        // Reduce the page.
                        let page = page.reduce();

                        // Get the logs and return the reduced page.
                        eth_node.get_logs(&filter).await.map(|logs| {
                            Some((
                                DownloadedLogs {
                                    start_height: oldest_block,
                                    last_height: latest_block,
                                    logs,
                                },
                                page,
                            ))
                        })
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
    S: futures::Stream<Item = Result<DownloadedLogs, ProviderError>>,
{
    tokio::pin!(logs);
    while let Some(DownloadedLogs {
        start_height,
        last_height,
        logs: events,
    }) = logs.try_next().await?
    {
        let mut unordered_events = HashMap::<DaBlockHeight, Vec<Event>>::new();
        let sorted_events = sort_events_by_log_index(events)?;
        let fuel_events = sorted_events.into_iter().filter_map(|event| {
            match EthEventLog::try_from(&event) {
                Ok(event) => {
                    match event {
                        EthEventLog::Message(m) => {
                            Some(Ok(Event::Message(Message::from(&m))))
                        }
                        EthEventLog::Transaction(tx) => {
                            Some(Ok(Event::Transaction(RelayedTransaction::from(tx))))
                        }
                        // TODO: Log out ignored messages.
                        EthEventLog::Ignored => None,
                    }
                }
                Err(e) => Some(Err(e)),
            }
        });

        for event in fuel_events {
            let event = event?;
            let height = event.da_height();
            unordered_events.entry(height).or_default().push(event);
        }

        let empty_events = Vec::new();
        for height in start_height..=last_height {
            let height: DaBlockHeight = height.into();
            let events = unordered_events.get(&height).unwrap_or(&empty_events);
            database.insert_events(&height, events)?;
        }
    }
    Ok(())
}

fn sort_events_by_log_index(events: Vec<Log>) -> anyhow::Result<Vec<Log>> {
    let mut with_indexes = events
        .into_iter()
        .map(|e| {
            let log_index = e
                .log_index
                .ok_or(anyhow::anyhow!("Log missing `log_index`: {e:?}"))?;
            Ok((log_index, e))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    with_indexes.sort_by(|(index_a, _a), (index_b, _b)| index_a.cmp(index_b));
    let new_events = with_indexes.into_iter().map(|(_, e)| e).collect();
    Ok(new_events)
}
