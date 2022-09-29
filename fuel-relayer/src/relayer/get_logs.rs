use super::*;

pub(crate) async fn download_logs<P>(
    eth_sync_gap: &state::EthSyncGap,
    contracts: Vec<H160>,
    eth_node: &P,
) -> Result<Vec<Log>, ProviderError>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    let filter = Filter::new()
        .from_block(eth_sync_gap.oldest())
        .to_block(eth_sync_gap.latest())
        .address(ValueOrArray::Array(contracts));

    eth_node.get_logs(&filter).await
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
