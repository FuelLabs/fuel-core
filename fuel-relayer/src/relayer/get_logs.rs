use super::*;

#[cfg(test)]
mod test;

pub(crate) async fn download_logs<P>(
    eth_sync_gap: &state::EthSyncGap,
    contracts: Vec<H160>,
    eth_node: Arc<P>,
    page_size: u64,
) -> Result<Vec<Log>, ProviderError>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    use futures::TryStreamExt;

    futures::stream::try_unfold(
        eth_sync_gap.page(page_size),
        |mut page: state::EthSyncPage| {
            let contracts = contracts.clone();
            let eth_node = eth_node.clone();
            async move {
                if page.is_empty() {
                    Ok(None)
                } else {
                    let filter = Filter::new()
                        .from_block(page.oldest())
                        .to_block(page.latest())
                        .address(ValueOrArray::Array(contracts));

                    page.reduce();

                    eth_node
                        .get_logs(&filter)
                        .await
                        .map(|logs| Some((logs, page)))
                }
            }
        },
    )
    .try_concat()
    .await
}

pub(crate) async fn write_logs(
    database: &mut dyn RelayerDb,
    logs: Vec<Log>,
) -> anyhow::Result<()> {
    let events: Vec<EthEventLog> = logs
        .iter()
        .map(|l| l.try_into())
        .collect::<Result<_, _>>()?;
    for event in events {
        match event {
            EthEventLog::Message(m) => {
                use fuel_core_interfaces::common::fuel_storage::StorageMutate;
                let m: Message = (&m).into();
                // Add messages to database
                StorageMutate::<Messages>::insert(database, &m.id(), &m)?;
            }
            EthEventLog::Ignored => (),
        }
    }
    Ok(())
}
