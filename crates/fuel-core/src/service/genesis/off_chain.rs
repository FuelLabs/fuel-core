use std::borrow::Cow;

use crate::{
    combined_database::CombinedDatabase,
    database::{
        database_description::off_chain::OffChain,
        Database,
    },
    graphql_api::{
        storage::{
            blocks::FuelBlockIdsToHeights,
            coins::OwnedCoins,
            contracts::ContractsInfo,
            messages::OwnedMessageIds,
            transactions::{
                OwnedTransactions,
                TransactionStatuses,
            },
        },
        worker_service,
    },
};
use fuel_core_chain_config::{
    SnapshotReader,
    TableEntry,
};
use fuel_core_storage::{
    tables::{
        Coins,
        Messages,
        Transactions,
    },
    transactional::StorageTransaction,
    StorageAsMut,
};
use fuel_core_types::services::executor::Event;

use super::{
    runner::ProcessState,
    workers::{
        GenesisWorkers,
        Handler,
    },
};

pub async fn import_state(
    db: CombinedDatabase,
    snapshot_reader: SnapshotReader,
) -> anyhow::Result<()> {
    let mut workers = GenesisWorkers::new(db, snapshot_reader);
    if let Err(e) = workers.run_off_chain_imports().await {
        workers.shutdown();
        workers.finished().await;

        return Err(e);
    }
    Ok(())
}

impl ProcessState for Handler<TransactionStatuses> {
    type TableInSnapshot = TransactionStatuses;
    type TableBeingWritten = TransactionStatuses;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut Database<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for tx_status in group {
            tx.storage::<Self::TableInSnapshot>()
                .insert(&tx_status.key, &tx_status.value)?;
        }
        Ok(())
    }
}

impl ProcessState for Handler<FuelBlockIdsToHeights> {
    type TableInSnapshot = FuelBlockIdsToHeights;
    type TableBeingWritten = FuelBlockIdsToHeights;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut Database<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for entry in group {
            tx.storage::<Self::TableInSnapshot>()
                .insert(&entry.key, &entry.value)?;
        }
        Ok(())
    }
}

impl ProcessState for Handler<OwnedTransactions> {
    type TableInSnapshot = OwnedTransactions;
    type TableBeingWritten = OwnedTransactions;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut Database<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for entry in group {
            tx.storage::<OwnedTransactions>()
                .insert(&entry.key, &entry.value)?;
        }
        Ok(())
    }
}

impl ProcessState for Handler<OwnedMessageIds> {
    type TableInSnapshot = Messages;
    type TableBeingWritten = OwnedMessageIds;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut Database<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let events = group
            .into_iter()
            .map(|TableEntry { value, .. }| Cow::Owned(Event::MessageImported(value)));
        worker_service::process_executor_events(events, tx)?;
        Ok(())
    }
}

impl ProcessState for Handler<OwnedCoins> {
    type TableInSnapshot = Coins;
    type TableBeingWritten = OwnedCoins;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut Database<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let events = group.into_iter().map(|TableEntry { value, key }| {
            Cow::Owned(Event::CoinCreated(value.uncompress(key)))
        });
        worker_service::process_executor_events(events, tx)?;
        Ok(())
    }
}

impl ProcessState for Handler<ContractsInfo> {
    type TableInSnapshot = Transactions;
    type TableBeingWritten = ContractsInfo;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut Database<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let transactions = group.iter().map(|TableEntry { value, .. }| value);
        worker_service::process_transactions(transactions, tx)?;
        Ok(())
    }
}
