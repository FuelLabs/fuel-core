use std::sync::{
    atomic::AtomicBool,
    Arc,
};

use super::{
    init_coin,
    init_contract,
    init_da_message,
    runner::{
        HandlesGenesisResource,
        ProcessState,
        ProcessStateGroup,
    },
    GenesisRunner,
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

pub struct GenesisWorkers<'a> {
    db: Database,
    stop_signal: Arc<AtomicBool>,
    block_height: BlockHeight,
    state_reader: &'a StateReader,
}

impl<'a> GenesisWorkers<'a> {
    pub fn new(
        db: Database,
        stop_signal: Arc<AtomicBool>,
        block_height: BlockHeight,
        state_reader: &'a StateReader,
    ) -> Self {
        Self {
            db,
            stop_signal,
            block_height,
            state_reader,
        }
    }

    pub fn coins(
        &self,
    ) -> GenesisRunner<
        impl ProcessStateGroup<CoinConfig>,
        impl Iterator<Item = anyhow::Result<Group<CoinConfig>>>,
        Database,
    > {
        let coins = self.state_reader.coins().unwrap();
        self.worker(coins)
    }

    pub fn messages(
        &self,
    ) -> GenesisRunner<
        impl ProcessStateGroup<MessageConfig>,
        impl Iterator<Item = anyhow::Result<Group<MessageConfig>>>,
        Database,
    > {
        let messages = self.state_reader.messages().unwrap();
        self.worker(messages)
    }

    pub fn contracts(
        &self,
    ) -> GenesisRunner<
        impl ProcessStateGroup<ContractConfig>,
        impl Iterator<Item = anyhow::Result<Group<ContractConfig>>>,
        Database,
    > {
        let contracts = self.state_reader.contracts().unwrap();
        self.worker(contracts)
    }

    pub fn contract_state(
        &self,
    ) -> GenesisRunner<
        impl ProcessStateGroup<ContractStateConfig>,
        impl Iterator<Item = anyhow::Result<Group<ContractStateConfig>>>,
        Database,
    > {
        let contract_state = self.state_reader.contract_state().unwrap();
        self.worker(contract_state)
    }

    pub fn contract_balance(
        &self,
    ) -> GenesisRunner<
        impl ProcessStateGroup<ContractBalanceConfig>,
        impl Iterator<Item = anyhow::Result<Group<ContractBalanceConfig>>>,
        Database,
    > {
        let contract_balance = self.state_reader.contract_balance().unwrap();
        self.worker(contract_balance)
    }

    fn worker<T, I>(&self, data: I) -> GenesisRunner<Handler, I, Database>
    where
        Handler: ProcessStateGroup<T>,
        T: HandlesGenesisResource,
        I: Iterator<Item = anyhow::Result<Group<T>>>,
    {
        let handler = Handler::new(self.block_height);
        let database = self.db.clone();
        let signal = Arc::clone(&self.stop_signal);
        GenesisRunner::new(signal, handler, data, database)
    }
}

#[derive(Debug, Clone, Copy)]
struct Handler {
    output_index: u64,
    block_height: BlockHeight,
}

impl Handler {
    fn new(block_height: BlockHeight) -> Self {
        Self {
            output_index: 0,
            block_height,
        }
    }
}

impl HandlesGenesisResource for CoinConfig {
    fn genesis_resource() -> GenesisResource {
        GenesisResource::Coins
    }
}

impl HandlesGenesisResource for MessageConfig {
    fn genesis_resource() -> GenesisResource {
        GenesisResource::Messages
    }
}

impl HandlesGenesisResource for ContractConfig {
    fn genesis_resource() -> GenesisResource {
        GenesisResource::Contracts
    }
}

impl HandlesGenesisResource for ContractStateConfig {
    fn genesis_resource() -> GenesisResource {
        GenesisResource::ContractStates
    }
}

impl HandlesGenesisResource for ContractBalanceConfig {
    fn genesis_resource() -> GenesisResource {
        GenesisResource::ContractBalances
    }
}

impl ProcessState<CoinConfig> for Handler {
    fn process(&mut self, coin: CoinConfig, tx: &mut Database) -> anyhow::Result<()> {
        let root = init_coin(tx, &coin, self.output_index, self.block_height)?;
        tx.add_coin_root(root)?;

        self.output_index = self.output_index
                .checked_add(1)
                .expect("The maximum number of UTXOs supported in the genesis configuration has been exceeded.");

        Ok(())
    }
}

impl ProcessState<MessageConfig> for Handler {
    fn process(
        &mut self,
        message: MessageConfig,
        tx: &mut Database,
    ) -> anyhow::Result<()> {
        let root = init_da_message(tx, &message)?;
        tx.add_message_root(root)?;
        Ok(())
    }
}

impl ProcessState<ContractConfig> for Handler {
    fn process(
        &mut self,
        contract: ContractConfig,
        tx: &mut Database,
    ) -> anyhow::Result<()> {
        init_contract(tx, &contract, self.output_index, self.block_height)?;
        tx.add_contract_id(contract.contract_id)?;

        self.output_index = self.output_index
                .checked_add(1)
                .expect("The maximum number of UTXOs supported in the genesis configuration has been exceeded.");

        Ok::<(), anyhow::Error>(())
    }
}

impl ProcessStateGroup<ContractStateConfig> for Handler {
    fn process_group(
        &mut self,
        group: Vec<ContractStateConfig>,
        tx: &mut Database,
    ) -> anyhow::Result<()> {
        tx.update_contract_states(group)?;
        Ok(())
    }
}

impl ProcessStateGroup<ContractBalanceConfig> for Handler {
    fn process_group(
        &mut self,
        group: Vec<ContractBalanceConfig>,
        tx: &mut Database,
    ) -> anyhow::Result<()> {
        tx.update_contract_balances(group)?;
        Ok(())
    }
}
