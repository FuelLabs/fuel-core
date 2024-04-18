use std::borrow::Cow;

use super::{
    import_task::ImportTable,
    Handler,
};
use crate::{
    database::{
        balances::BalancesInitializer,
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
        },
        state::StateInitializer,
        Database,
    },
    graphql_api::{
        storage::{
            blocks::FuelBlockIdsToHeights,
            transactions::{
                OwnedTransactions,
                TransactionStatuses,
            },
        },
        worker_service,
    },
};
use anyhow::anyhow;
use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsAssets,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        Messages,
        Transactions,
    },
    transactional::StorageTransaction,
    StorageAsMut,
};
use fuel_core_types::{
    self,
    blockchain::primitives::DaBlockHeight,
    entities::coins::coin::Coin,
    fuel_types::BlockHeight,
    services::executor::Event,
};

impl ImportTable<Coins> for Handler {
    fn on_chain(
        &mut self,
        group: Cow<Vec<TableEntry<Coins>>>,
        tx: &mut StorageTransaction<&mut Database<OnChain>>,
    ) -> anyhow::Result<()> {
        for coin in group.as_ref() {
            init_coin(tx, coin, self.block_height)?;
        }

        Ok(())
    }

    fn off_chain(
        &mut self,
        group: Cow<Vec<TableEntry<Coins>>>,
        tx: &mut StorageTransaction<&mut Database<OffChain>>,
    ) -> anyhow::Result<()> {
        let group = group.into_owned();
        let events = group.into_iter().map(|TableEntry { value, key }| {
            Cow::Owned(Event::CoinCreated(value.uncompress(key)))
        });
        worker_service::process_executor_events(events, tx)?;

        Ok(())
    }
}

impl ImportTable<Messages> for Handler {
    fn on_chain(
        &mut self,
        group: Cow<Vec<TableEntry<Messages>>>,
        tx: &mut StorageTransaction<&mut Database<OnChain>>,
    ) -> anyhow::Result<()> {
        for message in group.as_ref() {
            init_da_message(tx, message, self.da_block_height)?;
        }
        Ok(())
    }

    fn off_chain(
        &mut self,
        group: Cow<Vec<TableEntry<Messages>>>,
        tx: &mut StorageTransaction<&mut Database<OffChain>>,
    ) -> anyhow::Result<()> {
        let group = group.into_owned();
        let events = group
            .into_iter()
            .map(|TableEntry { value, .. }| Cow::Owned(Event::MessageImported(value)));
        worker_service::process_executor_events(events, tx)
    }
}

impl ImportTable<ContractsRawCode> for Handler {
    fn on_chain(
        &mut self,
        group: Cow<Vec<TableEntry<ContractsRawCode>>>,
        tx: &mut StorageTransaction<&mut Database<OnChain>>,
    ) -> anyhow::Result<()> {
        for contract in group.as_ref() {
            init_contract_raw_code(tx, contract)?;
        }
        Ok(())
    }
}

impl ImportTable<ContractsLatestUtxo> for Handler {
    fn on_chain(
        &mut self,
        group: Cow<Vec<TableEntry<ContractsLatestUtxo>>>,
        tx: &mut StorageTransaction<&mut Database<OnChain>>,
    ) -> anyhow::Result<()> {
        for utxo in group.as_ref() {
            init_contract_latest_utxo(tx, utxo, self.block_height)?;
        }
        Ok(())
    }
}

impl ImportTable<ContractsState> for Handler {
    fn on_chain(
        &mut self,
        group: Cow<Vec<TableEntry<ContractsState>>>,
        tx: &mut StorageTransaction<&mut Database<OnChain>>,
    ) -> anyhow::Result<()> {
        tx.update_contract_states(group.into_owned())?;
        Ok(())
    }
}

impl ImportTable<ContractsAssets> for Handler {
    fn on_chain(
        &mut self,
        group: Cow<Vec<TableEntry<ContractsAssets>>>,
        tx: &mut StorageTransaction<&mut Database<OnChain>>,
    ) -> anyhow::Result<()> {
        tx.update_contract_balances(group.into_owned())?;
        Ok(())
    }
}

impl ImportTable<Transactions> for Handler {
    fn on_chain(
        &mut self,
        group: Cow<Vec<TableEntry<Transactions>>>,
        tx: &mut StorageTransaction<&mut Database<OnChain>>,
    ) -> anyhow::Result<()> {
        for transaction in group.as_ref() {
            tx.storage::<Transactions>()
                .insert(&transaction.key, &transaction.value)?;
        }

        Ok(())
    }

    fn off_chain(
        &mut self,
        group: Cow<Vec<TableEntry<Transactions>>>,
        tx: &mut StorageTransaction<&mut Database<OffChain>>,
    ) -> anyhow::Result<()> {
        let transactions = group.iter().map(|TableEntry { value, .. }| value);
        worker_service::process_transactions(transactions, tx)?;
        Ok(())
    }
}

impl ImportTable<TransactionStatuses> for Handler {
    fn off_chain(
        &mut self,
        group: Cow<Vec<TableEntry<TransactionStatuses>>>,
        tx: &mut StorageTransaction<&mut Database<OffChain>>,
    ) -> anyhow::Result<()> {
        for tx_status in group.as_ref() {
            tx.storage::<TransactionStatuses>()
                .insert(&tx_status.key, &tx_status.value)?;
        }
        Ok(())
    }
}

impl ImportTable<FuelBlockIdsToHeights> for Handler {
    fn off_chain(
        &mut self,
        group: Cow<Vec<TableEntry<FuelBlockIdsToHeights>>>,
        tx: &mut StorageTransaction<&mut Database<OffChain>>,
    ) -> anyhow::Result<()> {
        for entry in group.as_ref() {
            tx.storage::<FuelBlockIdsToHeights>()
                .insert(&entry.key, &entry.value)?;
        }
        Ok(())
    }
}

impl ImportTable<OwnedTransactions> for Handler {
    fn off_chain(
        &mut self,
        group: Cow<Vec<TableEntry<OwnedTransactions>>>,
        tx: &mut StorageTransaction<&mut Database<OffChain>>,
    ) -> anyhow::Result<()> {
        for entry in group.as_ref() {
            tx.storage::<OwnedTransactions>()
                .insert(&entry.key, &entry.value)?;
        }
        Ok(())
    }
}

fn init_coin(
    transaction: &mut StorageTransaction<&mut Database>,
    coin: &TableEntry<Coins>,
    height: BlockHeight,
) -> anyhow::Result<()> {
    let utxo_id = coin.key;

    let compressed_coin = Coin {
        utxo_id,
        owner: *coin.value.owner(),
        amount: *coin.value.amount(),
        asset_id: *coin.value.asset_id(),
        tx_pointer: *coin.value.tx_pointer(),
    }
    .compress();

    // ensure coin can't point to blocks in the future
    let coin_height = coin.value.tx_pointer().block_height();
    if coin_height > height {
        return Err(anyhow!(
            "coin tx_pointer height ({coin_height}) cannot be greater than genesis block ({height})"
        ));
    }

    if transaction
        .storage::<Coins>()
        .insert(&utxo_id, &compressed_coin)?
        .is_some()
    {
        return Err(anyhow!("Coin should not exist"));
    }

    Ok(())
}

fn init_contract_latest_utxo(
    transaction: &mut StorageTransaction<&mut Database>,
    entry: &TableEntry<ContractsLatestUtxo>,
    height: BlockHeight,
) -> anyhow::Result<()> {
    let contract_id = entry.key;

    if entry.value.tx_pointer().block_height() > height {
        return Err(anyhow!(
            "contract tx_pointer cannot be greater than genesis block"
        ));
    }

    if transaction
        .storage::<ContractsLatestUtxo>()
        .insert(&contract_id, &entry.value)?
        .is_some()
    {
        return Err(anyhow!("Contract utxo should not exist"));
    }

    Ok(())
}

fn init_contract_raw_code(
    transaction: &mut StorageTransaction<&mut Database>,
    entry: &TableEntry<ContractsRawCode>,
) -> anyhow::Result<()> {
    let contract = entry.value.as_ref();
    let contract_id = entry.key;

    // insert contract code
    if transaction
        .storage::<ContractsRawCode>()
        .insert(&contract_id, contract)?
        .is_some()
    {
        return Err(anyhow!("Contract code should not exist"));
    }

    Ok(())
}

fn init_da_message(
    transaction: &mut StorageTransaction<&mut Database>,
    msg: &TableEntry<Messages>,
    da_height: DaBlockHeight,
) -> anyhow::Result<()> {
    let message = &msg.value;

    if message.da_height() > da_height {
        return Err(anyhow!(
            "message da_height cannot be greater than genesis da block height"
        ));
    }

    if transaction
        .storage::<Messages>()
        .insert(message.id(), message)?
        .is_some()
    {
        return Err(anyhow!("Message should not exist"));
    }

    Ok(())
}
