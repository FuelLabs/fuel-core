use super::{
    init_coin,
    init_contract,
    init_da_message,
    runner::ProcessState,
    GenesisRunner,
};
use std::{
    marker::PhantomData,
    sync::Arc,
};

use crate::database::{
    genesis_progress::GenesisResource,
    Database,
};
use fuel_core_chain_config::{
    CoinConfig,
    ContractBalanceConfig,
    ContractConfig,
    ContractStateConfig,
    Group,
    MessageConfig,
    StateReader,
};
use fuel_core_types::fuel_types::BlockHeight;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

struct FinishedWorkerSignals {
    coins: Arc<Notify>,
    messages: Arc<Notify>,
    contracts: Arc<Notify>,
    contract_state: Arc<Notify>,
    contract_balance: Arc<Notify>,
}

impl FinishedWorkerSignals {
    fn new() -> Self {
        Self {
            coins: Arc::new(Notify::new()),
            messages: Arc::new(Notify::new()),
            contracts: Arc::new(Notify::new()),
            contract_state: Arc::new(Notify::new()),
            contract_balance: Arc::new(Notify::new()),
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
            &self.contracts,
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
    state_reader: StateReader,
    finished_signals: FinishedWorkerSignals,
}

impl GenesisWorkers {
    pub fn new(db: Database, state_reader: StateReader) -> Self {
        let block_height = state_reader.block_height();
        Self {
            db,
            cancel_token: CancellationToken::new(),
            block_height,
            state_reader,
            finished_signals: FinishedWorkerSignals::new(),
        }
    }

    pub async fn run_imports(&self) -> anyhow::Result<()> {
        tokio::try_join!(
            self.spawn_coins_worker(),
            self.spawn_messages_worker(),
            self.spawn_contracts_worker(),
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
        let coins = self.state_reader.coins()?;
        let finished_signal = Arc::clone(&self.finished_signals.coins);
        self.spawn_worker(coins, finished_signal).await
    }

    async fn spawn_messages_worker(&self) -> anyhow::Result<()> {
        let messages = self.state_reader.messages()?;
        let finished_signal = Arc::clone(&self.finished_signals.messages);
        self.spawn_worker(messages, finished_signal).await
    }

    async fn spawn_contracts_worker(&self) -> anyhow::Result<()> {
        let contracts = self.state_reader.contracts()?;
        let finished_signal = Arc::clone(&self.finished_signals.contracts);
        self.spawn_worker(contracts, finished_signal).await
    }

    async fn spawn_contract_state_worker(&self) -> anyhow::Result<()> {
        let contract_state = self.state_reader.contract_state()?;
        let finished_signal = Arc::clone(&self.finished_signals.contract_state);
        self.spawn_worker(contract_state, finished_signal).await
    }

    async fn spawn_contract_balance_worker(&self) -> anyhow::Result<()> {
        let contract_balance = self.state_reader.contract_balance()?;
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
        let handler = Handler::new(self.block_height);
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
    // TODO: Remove this as part of the https://github.com/FuelLabs/fuel-core/issues/1668.
    //  Currently if we interrupt the regenesis process, we will use incorrect values.
    output_index: u64,
    block_height: BlockHeight,
    phaton_data: PhantomData<T>,
}

impl<T> Handler<T> {
    pub fn new(block_height: BlockHeight) -> Self {
        Self {
            output_index: 0,
            block_height,
            phaton_data: PhantomData,
        }
    }
}

impl ProcessState for Handler<CoinConfig> {
    type Item = CoinConfig;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut Database,
    ) -> anyhow::Result<()> {
        group
            .into_iter()
            .try_for_each(|coin| {
                init_coin(tx, &coin, self.output_index, self.block_height)?;

                self.output_index = self.output_index
                        .checked_add(1)
                        .expect("The maximum number of UTXOs supported in the genesis configuration has been exceeded.");

                Ok(())
            })
    }

    fn genesis_resource() -> GenesisResource {
        GenesisResource::Coins
    }
}

impl ProcessState for Handler<MessageConfig> {
    type Item = MessageConfig;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut Database,
    ) -> anyhow::Result<()> {
        group
            .into_iter()
            .try_for_each(|message| init_da_message(tx, message))
    }

    fn genesis_resource() -> GenesisResource {
        GenesisResource::Messages
    }
}

impl ProcessState for Handler<ContractConfig> {
    type Item = ContractConfig;

    fn process(
        &mut self,
        group: Vec<Self::Item>,
        tx: &mut Database,
    ) -> anyhow::Result<()> {
        group
            .into_iter()
            .try_for_each(|contract| {
                init_contract(tx, &contract, self.output_index, self.block_height)?;

                self.output_index = self.output_index
                        .checked_add(1)
                        .expect("The maximum number of UTXOs supported in the genesis configuration has been exceeded.");

                Ok(())
            })
    }

    fn genesis_resource() -> GenesisResource {
        GenesisResource::Contracts
    }
}

impl ProcessState for Handler<ContractStateConfig> {
    type Item = ContractStateConfig;

    fn process(
        &mut self,
        group: Vec<ContractStateConfig>,
        tx: &mut Database,
    ) -> anyhow::Result<()> {
        tx.update_contract_states(group)?;
        Ok(())
    }

    fn genesis_resource() -> GenesisResource {
        GenesisResource::ContractStates
    }
}

impl ProcessState for Handler<ContractBalanceConfig> {
    type Item = ContractBalanceConfig;

    fn process(
        &mut self,
        group: Vec<ContractBalanceConfig>,
        tx: &mut Database,
    ) -> anyhow::Result<()> {
        tx.update_contract_balances(group)?;
        Ok(())
    }

    fn genesis_resource() -> GenesisResource {
        GenesisResource::ContractBalances
    }
}
