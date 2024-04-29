use std::borrow::Cow;

use crate::{
    database::{
        database_description::off_chain::OffChain,
        GenesisDatabase,
    },
    fuel_core_graphql_api::storage::messages::SpentMessages,
    graphql_api::{
        storage::{
            blocks::FuelBlockIdsToHeights,
            coins::OwnedCoins,
            contracts::ContractsInfo,
            messages::OwnedMessageIds,
            old::{
                OldFuelBlockConsensus,
                OldFuelBlocks,
                OldTransactions,
            },
            transactions::{
                OwnedTransactions,
                TransactionStatuses,
            },
        },
        worker_service,
    },
};
use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    tables::{
        Coins,
        FuelBlocks,
        Messages,
        SealedBlockConsensus,
        Transactions,
    },
    transactional::StorageTransaction,
    StorageAsMut,
};
use fuel_core_types::services::executor::Event;

use super::{
    import_task::ImportTable,
    Handler,
};

impl ImportTable for Handler<TransactionStatuses, TransactionStatuses> {
    type TableInSnapshot = TransactionStatuses;
    type TableBeingWritten = TransactionStatuses;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for tx_status in group {
            tx.storage::<Self::TableInSnapshot>()
                .insert(&tx_status.key, &tx_status.value)?;
        }
        Ok(())
    }
}

impl ImportTable for Handler<FuelBlockIdsToHeights, FuelBlockIdsToHeights> {
    type TableInSnapshot = FuelBlockIdsToHeights;
    type TableBeingWritten = FuelBlockIdsToHeights;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for entry in group {
            tx.storage::<Self::TableInSnapshot>()
                .insert(&entry.key, &entry.value)?;
        }
        Ok(())
    }
}

impl ImportTable for Handler<OwnedTransactions, OwnedTransactions> {
    type TableInSnapshot = OwnedTransactions;
    type TableBeingWritten = OwnedTransactions;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for entry in group {
            tx.storage::<OwnedTransactions>()
                .insert(&entry.key, &entry.value)?;
        }
        Ok(())
    }
}

impl ImportTable for Handler<OwnedMessageIds, Messages> {
    type TableInSnapshot = Messages;
    type TableBeingWritten = OwnedMessageIds;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let events = group
            .into_iter()
            .map(|TableEntry { value, .. }| Cow::Owned(Event::MessageImported(value)));
        worker_service::process_executor_events(events, tx)?;
        Ok(())
    }
}

impl ImportTable for Handler<OwnedCoins, Coins> {
    type TableInSnapshot = Coins;
    type TableBeingWritten = OwnedCoins;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let events = group.into_iter().map(|TableEntry { value, key }| {
            Cow::Owned(Event::CoinCreated(value.uncompress(key)))
        });
        worker_service::process_executor_events(events, tx)?;
        Ok(())
    }
}

impl ImportTable for Handler<ContractsInfo, Transactions> {
    type TableInSnapshot = Transactions;
    type TableBeingWritten = ContractsInfo;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let transactions = group.iter().map(|TableEntry { value, .. }| value);
        worker_service::process_transactions(transactions, tx)?;
        Ok(())
    }
}

impl ImportTable for Handler<ContractsInfo, OldTransactions> {
    type TableInSnapshot = OldTransactions;
    type TableBeingWritten = ContractsInfo;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let transactions = group.iter().map(|TableEntry { value, .. }| value);
        worker_service::process_transactions(transactions, tx)?;
        Ok(())
    }
}

impl ImportTable for Handler<OldFuelBlocks, FuelBlocks> {
    type TableInSnapshot = FuelBlocks;
    type TableBeingWritten = OldFuelBlocks;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let blocks = group
            .iter()
            .map(|TableEntry { key, value, .. }| (key, value));
        worker_service::copy_to_old_blocks(blocks, tx)?;
        Ok(())
    }
}

impl ImportTable for Handler<OldFuelBlocks, OldFuelBlocks> {
    type TableInSnapshot = OldFuelBlocks;
    type TableBeingWritten = OldFuelBlocks;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let blocks = group
            .iter()
            .map(|TableEntry { key, value, .. }| (key, value));
        worker_service::copy_to_old_blocks(blocks, tx)?;
        Ok(())
    }
}

impl ImportTable for Handler<OldFuelBlockConsensus, SealedBlockConsensus> {
    type TableInSnapshot = SealedBlockConsensus;
    type TableBeingWritten = OldFuelBlockConsensus;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let blocks = group
            .iter()
            .map(|TableEntry { key, value, .. }| (key, value));
        worker_service::copy_to_old_block_consensus(blocks, tx)?;
        Ok(())
    }
}

impl ImportTable for Handler<OldFuelBlockConsensus, OldFuelBlockConsensus> {
    type TableInSnapshot = OldFuelBlockConsensus;
    type TableBeingWritten = OldFuelBlockConsensus;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let blocks = group
            .iter()
            .map(|TableEntry { key, value, .. }| (key, value));
        worker_service::copy_to_old_block_consensus(blocks, tx)?;
        Ok(())
    }
}

impl ImportTable for Handler<OldTransactions, Transactions> {
    type TableInSnapshot = Transactions;
    type TableBeingWritten = OldTransactions;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let transactions = group
            .iter()
            .map(|TableEntry { key, value, .. }| (key, value));
        worker_service::copy_to_old_transactions(transactions, tx)?;
        Ok(())
    }
}

impl ImportTable for Handler<OldTransactions, OldTransactions> {
    type TableInSnapshot = OldTransactions;
    type TableBeingWritten = OldTransactions;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let transactions = group
            .iter()
            .map(|TableEntry { key, value, .. }| (key, value));
        worker_service::copy_to_old_transactions(transactions, tx)?;
        Ok(())
    }
}

impl ImportTable for Handler<SpentMessages, SpentMessages> {
    type TableInSnapshot = SpentMessages;
    type TableBeingWritten = SpentMessages;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for entry in group {
            tx.storage_as_mut::<SpentMessages>()
                .insert(&entry.key, &entry.value)?;
        }
        Ok(())
    }
}

impl ImportTable for Handler<FuelBlockIdsToHeights, FuelBlocks> {
    type TableInSnapshot = FuelBlocks;
    type TableBeingWritten = FuelBlockIdsToHeights;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for entry in group {
            tx.storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&entry.value.id(), &entry.key)?;
        }
        Ok(())
    }
}

impl ImportTable for Handler<FuelBlockIdsToHeights, OldFuelBlocks> {
    type TableInSnapshot = OldFuelBlocks;
    type TableBeingWritten = FuelBlockIdsToHeights;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for entry in group {
            tx.storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&entry.value.id(), &entry.key)?;
        }
        Ok(())
    }
}
