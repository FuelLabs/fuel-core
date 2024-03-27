use super::{
    init_coin,
    init_contract_latest_utxo,
    init_contract_raw_code,
    init_da_message,
    runner::ProcessState,
    GenesisRunner,
};
use std::{
    collections::HashMap,
    marker::PhantomData,
    sync::Arc,
};

use crate::{
    combined_database::CombinedDatabase,
    database::{
        balances::BalancesInitializer,
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
            DatabaseDescription,
        },
        state::StateInitializer,
        Database,
    },
    graphql_api::storage::transactions::{
        OwnedTransactions,
        TransactionStatuses,
    },
};
use fuel_core_chain_config::{
    AsTable,
    SnapshotReader,
    StateConfig,
    TableEntry,
};
use fuel_core_storage::{
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
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
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};
use tokio::sync::Notify;
use tokio_rayon::AsyncRayonHandle;
use tokio_util::sync::CancellationToken;

pub struct GenesisWorkers {
    db: CombinedDatabase,
    cancel_token: CancellationToken,
    block_height: BlockHeight,
    da_block_height: DaBlockHeight,
    snapshot_reader: SnapshotReader,
    finished_signals: HashMap<String, Arc<Notify>>,
}

impl GenesisWorkers {
    pub fn new(db: CombinedDatabase, snapshot_reader: SnapshotReader) -> Self {
        let block_height = snapshot_reader.block_height();
        let da_block_height = snapshot_reader.da_block_height();
        Self {
            db,
            cancel_token: CancellationToken::new(),
            block_height,
            da_block_height,
            snapshot_reader,
            finished_signals: HashMap::default(),
        }
    }

    pub async fn run_imports(&mut self) -> anyhow::Result<()> {
        tokio::try_join!(
            self.spawn_worker_on_chain::<Coins>()?,
            self.spawn_worker_on_chain::<Messages>()?,
            self.spawn_worker_on_chain::<ContractsRawCode>()?,
            self.spawn_worker_on_chain::<ContractsLatestUtxo>()?,
            self.spawn_worker_on_chain::<ContractsState>()?,
            self.spawn_worker_on_chain::<ContractsAssets>()?,
            self.spawn_worker_on_chain::<Transactions>()?,
            self.spawn_worker_off_chain::<TransactionStatuses>()?,
            self.spawn_worker_off_chain::<OwnedTransactions>()?,
        )
        .map(|_| ())
    }

    pub async fn finished(&self) {
        for signal in self.finished_signals.values() {
            signal.notified().await;
        }
    }

    pub fn shutdown(&self) {
        self.cancel_token.cancel()
    }

    pub fn spawn_worker_on_chain<T>(
        &mut self,
    ) -> anyhow::Result<AsyncRayonHandle<anyhow::Result<()>>>
    where
        T: TableWithBlueprint + Send + 'static,
        T::OwnedKey: serde::de::DeserializeOwned + Send,
        T::OwnedValue: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<T>,
        Handler<T>: ProcessState<Table = T, DbDesc = OnChain>,
    {
        let groups = self.snapshot_reader.read::<T>()?;
        let finished_signal = self.get_signal(T::column().name());

        let runner = GenesisRunner::new(
            Some(finished_signal),
            self.cancel_token.clone(),
            Handler::new(self.block_height, self.da_block_height),
            groups,
            self.db.on_chain().clone(),
        );
        Ok(tokio_rayon::spawn(move || runner.run()))
    }

    // TODO: serde bounds can be written shorter
    pub fn spawn_worker_off_chain<T>(
        &mut self,
    ) -> anyhow::Result<AsyncRayonHandle<anyhow::Result<()>>>
    where
        T: TableWithBlueprint<Column = <OffChain as DatabaseDescription>::Column>
            + Send
            + 'static,
        T::OwnedKey: serde::de::DeserializeOwned + Send,
        T::OwnedValue: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<T>,
        Handler<T>: ProcessState<Table = T, DbDesc = OffChain>,
    {
        let groups = self.snapshot_reader.read::<T>()?;
        let finished_signal = self.get_signal(T::column().name());
        let runner = GenesisRunner::new(
            Some(finished_signal),
            self.cancel_token.clone(),
            Handler::new(self.block_height, self.da_block_height),
            groups,
            self.db.off_chain().clone(),
        );
        Ok(tokio_rayon::spawn(move || runner.run()))
    }

    fn get_signal(&mut self, name: &str) -> Arc<Notify> {
        self.finished_signals
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Handler<T> {
    block_height: BlockHeight,
    da_block_height: DaBlockHeight,
    phaton_data: PhantomData<T>,
}

impl<T> Handler<T> {
    pub fn new(block_height: BlockHeight, da_block_height: DaBlockHeight) -> Self {
        Self {
            block_height,
            da_block_height,
            phaton_data: PhantomData,
        }
    }
}

impl ProcessState for Handler<Coins> {
    type Table = Coins;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::Table>>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        group.into_iter().try_for_each(|coin| {
            init_coin(tx, &coin, self.block_height)?;
            Ok(())
        })
    }
}

impl ProcessState for Handler<Messages> {
    type Table = Messages;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::Table>>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        group
            .into_iter()
            .try_for_each(|message| init_da_message(tx, message, self.da_block_height))
    }
}

impl ProcessState for Handler<ContractsRawCode> {
    type Table = ContractsRawCode;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::Table>>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        group.into_iter().try_for_each(|contract| {
            init_contract_raw_code(tx, &contract)?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

impl ProcessState for Handler<ContractsLatestUtxo> {
    type Table = ContractsLatestUtxo;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::Table>>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        group.into_iter().try_for_each(|contract| {
            init_contract_latest_utxo(tx, &contract, self.block_height)?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

impl ProcessState for Handler<ContractsState> {
    type Table = ContractsState;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::Table>>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        tx.update_contract_states(group)?;
        Ok(())
    }
}

impl ProcessState for Handler<ContractsAssets> {
    type Table = ContractsAssets;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::Table>>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        tx.update_contract_balances(group)?;
        Ok(())
    }
}

impl ProcessState for Handler<Transactions> {
    type Table = Transactions;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::Table>>,
        tx: &mut StorageTransaction<&mut Database<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for transaction in &group {
            tx.storage::<Transactions>()
                .insert(&transaction.key, &transaction.value)?;
        }
        Ok(())
    }
}

impl ProcessState for Handler<TransactionStatuses> {
    type Table = TransactionStatuses;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::Table>>,
        tx: &mut StorageTransaction<&mut Database<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for tx_status in group {
            tx.storage::<TransactionStatuses>()
                .insert(&tx_status.key, &tx_status.value)?;
        }
        Ok(())
    }
}

impl ProcessState for Handler<OwnedTransactions> {
    type Table = OwnedTransactions;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::Table>>,
        tx: &mut StorageTransaction<&mut Database<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for entry in group {
            tx.storage::<OwnedTransactions>()
                .insert(&entry.key, &entry.value)?;
        }
        Ok(())
    }
}
