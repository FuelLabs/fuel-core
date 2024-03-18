use super::{
    init_coin,
    init_contract_info,
    init_contract_latest_utxo,
    init_contract_raw_code,
    init_da_message,
    runner::ProcessState,
    GenesisRunner,
};
use std::{
    marker::PhantomData,
    sync::Arc,
};

use crate::database::{
    balances::BalancesInitializer,
    genesis_progress::GenesisResource,
    state::StateInitializer,
    Database,
};
use fuel_core_chain_config::{
    CoinConfig,
    ContractConfig,
    Group,
    MessageConfig,
    MyEntry,
    SnapshotReader,
};
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsAssets,
        ContractsInfo,
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
use tokio_util::sync::CancellationToken;

struct FinishedWorkerSignals {
    coins: Arc<Notify>,
    messages: Arc<Notify>,
    contracts_code: Arc<Notify>,
    contracts_info: Arc<Notify>,
    contracts_utxos: Arc<Notify>,
    contract_state: Arc<Notify>,
    contract_balance: Arc<Notify>,
}

impl FinishedWorkerSignals {
    fn new() -> Self {
        Self {
            coins: Arc::new(Notify::new()),
            messages: Arc::new(Notify::new()),
            contract_state: Arc::new(Notify::new()),
            contract_balance: Arc::new(Notify::new()),
            contracts_code: Arc::new(Notify::new()),
            contracts_info: Arc::new(Notify::new()),
            contracts_utxos: Arc::new(Notify::new()),
        }
    }
}

impl<'a> IntoIterator for &'a FinishedWorkerSignals {
    type Item = &'a Arc<Notify>;
    type IntoIter = std::vec::IntoIter<&'a Arc<Notify>>;

    fn into_iter(self) -> Self::IntoIter {
        vec![
            &self.coins,
            &self.messages,
            &self.contracts_code,
            &self.contracts_info,
            &self.contracts_utxos,
            &self.contract_state,
            &self.contract_balance,
        ]
        .into_iter()
    }
}

pub struct GenesisWorkers {
    db: Database,
    cancel_token: CancellationToken,
    block_height: BlockHeight,
    da_block_height: DaBlockHeight,
    state_reader: SnapshotReader,
    finished_signals: FinishedWorkerSignals,
}

impl GenesisWorkers {
    pub fn new(db: Database, state_reader: SnapshotReader) -> Self {
        let block_height = state_reader.block_height();
        let da_block_height = state_reader.da_block_height();
        Self {
            db,
            cancel_token: CancellationToken::new(),
            block_height,
            da_block_height,
            state_reader,
            finished_signals: FinishedWorkerSignals::new(),
        }
    }

    pub async fn run_imports(&self) -> anyhow::Result<()> {
        tokio::try_join!(
            self.spawn_coins_worker(),
            self.spawn_messages_worker(),
            self.spawn_contracts_code_worker(),
            self.spawn_contracts_info_worker(),
            self.spawn_contracts_utxo_worker(),
            self.spawn_contract_state_worker(),
            self.spawn_contract_balance_worker()
        )
        .map(|_| ())
    }

    pub async fn finished(&self) {
        for signal in &self.finished_signals {
            signal.notified().await;
        }
    }

    pub fn shutdown(&self) {
        self.cancel_token.cancel()
    }

    async fn spawn_coins_worker(&self) -> anyhow::Result<()> {
        let coins = self.state_reader.read::<Coins>()?;
        let finished_signal = Arc::clone(&self.finished_signals.coins);
        self.spawn_worker(coins, finished_signal).await
    }

    async fn spawn_messages_worker(&self) -> anyhow::Result<()> {
        let messages = self.state_reader.read::<Messages>()?;
        let finished_signal = Arc::clone(&self.finished_signals.messages);
        self.spawn_worker(messages, finished_signal).await
    }

    async fn spawn_contracts_code_worker(&self) -> anyhow::Result<()> {
        let contracts = self.state_reader.read::<ContractsRawCode>()?;
        let finished_signal = Arc::clone(&self.finished_signals.contracts_code);
        self.spawn_worker(contracts, finished_signal).await
    }

    async fn spawn_contracts_info_worker(&self) -> anyhow::Result<()> {
        let contracts = self.state_reader.read::<ContractsInfo>()?;
        let finished_signal = Arc::clone(&self.finished_signals.contracts_info);
        self.spawn_worker(contracts, finished_signal).await
    }

    async fn spawn_contracts_utxo_worker(&self) -> anyhow::Result<()> {
        let contracts = self.state_reader.read::<ContractsLatestUtxo>()?;
        let finished_signal = Arc::clone(&self.finished_signals.contracts_utxos);
        self.spawn_worker(contracts, finished_signal).await
    }

    async fn spawn_contract_state_worker(&self) -> anyhow::Result<()> {
        let contract_state = self.state_reader.read::<ContractsState>()?;
        let finished_signal = Arc::clone(&self.finished_signals.contract_state);
        self.spawn_worker(contract_state, finished_signal).await
    }

    async fn spawn_contract_balance_worker(&self) -> anyhow::Result<()> {
        let contract_balance = self.state_reader.read::<ContractsAssets>()?;
        let finished_signal = Arc::clone(&self.finished_signals.contract_balance);
        self.spawn_worker(contract_balance, finished_signal).await
    }

    fn spawn_worker<T, I>(
        &self,
        data: I,
        finished_signal: Arc<Notify>,
    ) -> tokio_rayon::AsyncRayonHandle<Result<(), anyhow::Error>>
    where
        T: Send + 'static,
        Handler<T>: ProcessState<Item = T>,
        I: IntoIterator<Item = anyhow::Result<Group<T>>> + Send + 'static,
    {
        let runner = self.create_runner(data, Some(finished_signal));
        tokio_rayon::spawn(move || runner.run())
    }

    fn create_runner<T, I>(
        &self,
        data: I,
        finished_signal: Option<Arc<Notify>>,
    ) -> GenesisRunner<Handler<T>, I, Database>
    where
        Handler<T>: ProcessState<Item = T>,
        I: IntoIterator<Item = anyhow::Result<Group<T>>>,
    {
        let handler = Handler::new(self.block_height, self.da_block_height);
        let database = self.db.clone();
        GenesisRunner::new(
            finished_signal,
            self.cancel_token.clone(),
            handler,
            data,
            database,
        )
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

impl ProcessState for Handler<MyEntry<Coins>> {
    type Item = MyEntry<Coins>;

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

    fn genesis_resource() -> GenesisResource {
        GenesisResource::Coins
    }
}

impl ProcessState for Handler<MyEntry<Messages>> {
    type Item = MyEntry<Messages>;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        group
            .into_iter()
            .try_for_each(|message| init_da_message(tx, message, self.da_block_height))
    }

    fn genesis_resource() -> GenesisResource {
        GenesisResource::Messages
    }
}

impl ProcessState for Handler<MyEntry<ContractsRawCode>> {
    type Item = MyEntry<ContractsRawCode>;

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

    fn genesis_resource() -> GenesisResource {
        GenesisResource::ContractsCode
    }
}

impl ProcessState for Handler<MyEntry<ContractsInfo>> {
    type Item = MyEntry<ContractsInfo>;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        group.into_iter().try_for_each(|contract| {
            init_contract_info(tx, &contract)?;
            Ok::<(), anyhow::Error>(())
        })
    }

    fn genesis_resource() -> GenesisResource {
        GenesisResource::ContractsInfo
    }
}

impl ProcessState for Handler<MyEntry<ContractsLatestUtxo>> {
    type Item = MyEntry<ContractsLatestUtxo>;

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

    fn genesis_resource() -> GenesisResource {
        GenesisResource::ContractsLatestUtxo
    }
}

impl ProcessState for Handler<MyEntry<ContractsState>> {
    type Item = MyEntry<ContractsState>;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        tx.update_contract_states(group)?;
        Ok(())
    }

    fn genesis_resource() -> GenesisResource {
        GenesisResource::ContractStates
    }
}

impl ProcessState for Handler<MyEntry<ContractsAssets>> {
    type Item = MyEntry<ContractsAssets>;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut StorageTransaction<&mut Database>,
    ) -> anyhow::Result<()> {
        tx.update_contract_balances(group)?;
        Ok(())
    }

    fn genesis_resource() -> GenesisResource {
        GenesisResource::ContractBalances
    }
}
