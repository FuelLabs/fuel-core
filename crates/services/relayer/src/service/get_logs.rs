use super::*;
use alloy_primitives::Address;
use alloy_provider::transport::{
    TransportError,
    TransportErrorKind,
};
use fuel_core_types::{
    entities::RelayedTransaction,
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
pub(crate) fn download_logs<'a, P, S>(
    eth_sync_gap: &state::EthSyncGap,
    contracts: Vec<Address>,
    eth_node: &'a P,
    page_sizer: &'a mut S,
) -> impl futures::Stream<Item = Result<DownloadedLogs, TransportError>> + 'a + use<'a, P, S>
where
    P: Provider + 'static,
    S: PageSizer + 'static + Send,
{
    // Create a stream of paginated logs.
    futures::stream::try_unfold(
        (eth_sync_gap.page(page_sizer.page_size()), page_sizer),
        move |(page, page_sizer)| {
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
                            .event_signature(ValueOrArray::Array(vec![
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

                        // Get the logs and return the reduced page.
                        eth_node
                            .get_logs(&filter)
                            .await
                            .map_err(|err| {
                                let TransportError::Transport(err) = err else {
                                    // We optimistically reduce the page size for any rpc error,
                                    // to reduce the work to check if it is actually
                                    // because of "too many logs"
                                    page_sizer.update(RpcOutcome::Error);
                                    return err;
                                };

                                // Workaround because `QuorumError` obfuscates useful information
                                TransportErrorKind::custom_str(&format!(
                                    "eth provider failed to get logs: {err:?}"
                                ))
                            })
                            .map(|logs| {
                                // First, update the adaptive page sizer, to have the updated page size.
                                page_sizer.update(RpcOutcome::Success {
                                    logs_downloaded: logs.len() as u64,
                                });

                                // Reduce the size, using the new page size.
                                let page =
                                    page.advance_and_resize(page_sizer.page_size());
                                Some((
                                    DownloadedLogs {
                                        start_height: oldest_block,
                                        last_height: latest_block,
                                        logs,
                                    },
                                    (page, page_sizer),
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
    S: futures::Stream<Item = Result<DownloadedLogs, TransportError>>,
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
