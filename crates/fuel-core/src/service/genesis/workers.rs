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
        },
        state::StateInitializer,
        Database,
    },
    graphql_api::storage::transactions::TransactionStatuses,
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
        Handler<TableEntry<T>>: ProcessState<Item = TableEntry<T>, DbDesc = OnChain>,
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

    pub fn spawn_worker_off_chain<T>(
        &mut self,
    ) -> anyhow::Result<AsyncRayonHandle<anyhow::Result<()>>>
    where
        T: TableWithBlueprint + Send + 'static,
        T::OwnedKey: serde::de::DeserializeOwned + Send,
        T::OwnedValue: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<T>,
        Handler<TableEntry<T>>: ProcessState<Item = TableEntry<T>, DbDesc = OffChain>,
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

impl ProcessState for Handler<TableEntry<Coins>> {
    type Item = TableEntry<Coins>;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        group.into_iter().try_for_each(|coin| {
            init_coin(tx, &coin, self.block_height)?;
            Ok(())
        })
    }

    fn genesis_resource() -> &'static str {
        Coins::column().name()
    }
}

impl ProcessState for Handler<TableEntry<Messages>> {
    type Item = TableEntry<Messages>;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        group
            .into_iter()
            .try_for_each(|message| init_da_message(tx, message, self.da_block_height))
    }

    fn genesis_resource() -> &'static str {
        Messages::column().name()
    }
}

impl ProcessState for Handler<TableEntry<ContractsRawCode>> {
    type Item = TableEntry<ContractsRawCode>;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        group.into_iter().try_for_each(|contract| {
            init_contract_raw_code(tx, &contract)?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn genesis_resource() -> &'static str {
        ContractsRawCode::column().name()
    }
}

impl ProcessState for Handler<TableEntry<ContractsLatestUtxo>> {
    type Item = TableEntry<ContractsLatestUtxo>;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        group.into_iter().try_for_each(|contract| {
            init_contract_latest_utxo(tx, &contract, self.block_height)?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn genesis_resource() -> &'static str {
        ContractsLatestUtxo::column().name()
    }
}

impl ProcessState for Handler<TableEntry<ContractsState>> {
    type Item = TableEntry<ContractsState>;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        tx.update_contract_states(group)?;
        Ok(())
    }

    fn genesis_resource() -> &'static str {
        ContractsLatestUtxo::column().name()
    }
}

impl ProcessState for Handler<TableEntry<ContractsAssets>> {
    type Item = TableEntry<ContractsAssets>;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        tx.update_contract_balances(group)?;
        Ok(())
    }

    fn genesis_resource() -> &'static str {
        ContractsAssets::column().name()
    }
}

impl ProcessState for Handler<TableEntry<Transactions>> {
    type Item = TableEntry<Transactions>;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        for transaction in &group {
            tx.storage::<Transactions>()
                .insert(&transaction.key, &transaction.value)?;
        }
        Ok(())
    }

    fn genesis_resource() -> &'static str {
        Transactions::column().name()
    }
}

impl ProcessState for Handler<TableEntry<TransactionStatuses>> {
    type Item = TableEntry<TransactionStatuses>;
    type DbDesc = OffChain;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut StorageTransaction<&mut Database<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        for tx_status in group {
            tx.storage::<TransactionStatuses>()
                .insert(&tx_status.key, &tx_status.value)?;
        }
        Ok(())
    }

    fn genesis_resource() -> &'static str {
        TransactionStatuses::column().name()
    }
}
