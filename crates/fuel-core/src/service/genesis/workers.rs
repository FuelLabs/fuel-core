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

use crate::database::{
    balances::BalancesInitializer,
    state::StateInitializer,
    Database,
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
    },
    transactional::StorageTransaction,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};
use tokio::sync::Notify;
use tokio_rayon::AsyncRayonHandle;
use tokio_util::sync::CancellationToken;

pub struct GenesisWorkers {
    db: Database,
    cancel_token: CancellationToken,
    block_height: BlockHeight,
    da_block_height: DaBlockHeight,
    snapshot_reader: SnapshotReader,
    finished_signals: HashMap<String, Arc<Notify>>,
}

impl GenesisWorkers {
    pub fn new(db: Database, snapshot_reader: SnapshotReader) -> Self {
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
            self.spawn_worker::<Coins>()?,
            self.spawn_worker::<Messages>()?,
            self.spawn_worker::<ContractsRawCode>()?,
            self.spawn_worker::<ContractsLatestUtxo>()?,
            self.spawn_worker::<ContractsState>()?,
            self.spawn_worker::<ContractsAssets>()?,
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

    pub fn spawn_worker<T>(
        &mut self,
    ) -> anyhow::Result<AsyncRayonHandle<anyhow::Result<()>>>
    where
        T: TableWithBlueprint + Send + 'static,
        T::OwnedKey: serde::de::DeserializeOwned + Send,
        T::OwnedValue: serde::de::DeserializeOwned + Send,
        StateConfig: AsTable<T>,
        Handler<T>: ProcessState<Table = T>,
    {
        let groups = self.snapshot_reader.read::<T>()?;
        let finished_signal = self.get_signal(T::column().name());
        let handler = Handler::new(self.block_height, self.da_block_height);

        let database = self.db.clone();
        let runner = GenesisRunner::new(
            Some(finished_signal),
            self.cancel_token.clone(),
            handler,
            groups,
            database,
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

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::Table>>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        tx.update_contract_balances(group)?;
        Ok(())
    }
}
