use crate::{
    database::{
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
        },
        genesis_progress::GenesisMetadata,
        Database,
    },
    graphql_api::{
        storage::{
            coins::OwnedCoins,
            contracts::ContractsInfo,
            messages::OwnedMessageIds,
        },
        worker_service,
    },
};
use fuel_core_storage::{
    blueprint::BlueprintInspect,
    iter::IteratorOverTable,
    kv_store::StorageColumn,
    structured_storage::{
        StructuredStorage,
        TableWithBlueprint,
    },
    tables::{
        Coins,
        Messages,
        Transactions,
    },
    transactional::{
        InMemoryTransaction,
        WriteTransaction,
    },
    Mappable,
    StorageAsMut,
};

use fuel_core_types::services::executor::Event;
use itertools::Itertools;
use std::borrow::Cow;

const CHUNK_SIZE: usize = 1024;

pub fn derive_offchain_table<Src, Dst, F>(
    on_chain_database: &Database<OnChain>,
    off_chain_database: &mut Database<OffChain>,
    mut callback: F,
) -> anyhow::Result<()>
where
    Src: TableWithBlueprint<Column = fuel_core_storage::column::Column>,
    Dst: TableWithBlueprint<Column = crate::graphql_api::storage::Column>,
    <Src as TableWithBlueprint>::Blueprint: BlueprintInspect<Src, Database<OnChain>>,
    F: FnMut(
        &mut StructuredStorage<InMemoryTransaction<&mut Database<OffChain>>>,
        Vec<(<Src as Mappable>::OwnedKey, <Src as Mappable>::OwnedValue)>,
    ) -> anyhow::Result<()>,
{
    let skip = match off_chain_database
        .storage::<GenesisMetadata<OffChain>>()
        .get(Dst::column().name())
    {
        Ok(Some(idx_last_handled)) => {
            usize::saturating_add(idx_last_handled.into_owned(), 1)
        }
        _ => 0,
    };

    for (chunk_index, chunk) in on_chain_database
        .iter_all::<Src>(None)
        .chunks(CHUNK_SIZE)
        .into_iter()
        .enumerate()
        .skip(skip)
    {
        let chunk: Vec<_> = chunk.try_collect()?;

        let mut tx = off_chain_database.write_transaction();

        callback(&mut tx, chunk)?;

        tx.storage_as_mut::<GenesisMetadata<OffChain>>()
            .insert(Dst::column().name(), &chunk_index)?;

        tx.commit()?;
    }
    Ok(())
}

/// Performs the importing of the genesis block from the snapshot.
// TODO: The regenesis of the off-chain database should go in the same way as the on-chain database.
//  https://github.com/FuelLabs/fuel-core/issues/1619
pub fn execute_genesis_block(
    on_chain_database: &Database<OnChain>,
    off_chain_database: &mut Database<OffChain>,
) -> anyhow::Result<()> {
    derive_offchain_table::<Messages, OwnedMessageIds, _>(
        on_chain_database,
        off_chain_database,
        |tx, chunk| {
            let events = chunk
                .into_iter()
                .map(|(_, message)| Cow::Owned(Event::MessageImported(message)));
            worker_service::process_executor_events(events, tx)
        },
    )?;

    derive_offchain_table::<Coins, OwnedCoins, _>(
        on_chain_database,
        off_chain_database,
        |tx, chunk| {
            let events = chunk.into_iter().map(|(utxo_id, coin)| {
                let coin = coin.uncompress(utxo_id);
                Cow::Owned(Event::CoinCreated(coin))
            });
            worker_service::process_executor_events(events, tx)
        },
    )?;

    derive_offchain_table::<Transactions, ContractsInfo, _>(
        on_chain_database,
        off_chain_database,
        |tx, chunk| {
            let transactions = chunk.iter().map(|(_, tx)| tx);
            worker_service::process_transactions(transactions, tx)?;
            Ok(())
        },
    )?;

    Ok(())
}
