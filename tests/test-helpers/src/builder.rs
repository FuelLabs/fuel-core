use fuel_core::{
    chain_config::{
        ChainConfig,
        CoinConfig,
        ContractConfig,
        StateConfig,
    },
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::FuelClient;
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_asm::op,
    fuel_tx::{
        field::Inputs,
        input::coin::{
            CoinPredicate,
            CoinSigned,
        },
        policies::Policies,
        *,
    },
    fuel_types::BlockHeight,
};
use itertools::Itertools;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::{
    collections::HashMap,
    io,
};

/// Helper for wrapping a currently running node environment
pub struct TestContext {
    pub srv: FuelService,
    pub rng: StdRng,
    pub client: FuelClient,
}

impl TestContext {
    pub async fn new(seed: u64) -> Self {
        let rng = StdRng::seed_from_u64(seed);
        let srv = FuelService::new_node(Config::local_node()).await.unwrap();
        let client = FuelClient::from(srv.bound_address);
        Self { srv, rng, client }
    }

    pub async fn transfer(
        &mut self,
        from: Address,
        to: Address,
        amount: u64,
    ) -> io::Result<Bytes32> {
        let script = op::ret(0x10).to_bytes().to_vec();
        let tx = Transaction::script(
            1_000_000,
            script,
            vec![],
            Policies::new().with_gas_price(0),
            vec![Input::coin_signed(
                self.rng.gen(),
                from,
                amount,
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            )],
            vec![Output::coin(to, amount, Default::default())],
            vec![vec![].into()],
        )
        .into();
        self.client.submit_and_await_commit(&tx).await?;
        Ok(tx.id(&Default::default()))
    }
}

/// Helper for configuring the genesis block in tests
#[derive(Clone)]
pub struct TestSetupBuilder {
    pub rng: StdRng,
    pub contracts: HashMap<ContractId, ContractConfig>,
    pub initial_coins: Vec<CoinConfig>,
    pub min_gas_price: u64,
    pub gas_limit: u64,
    pub starting_block: Option<BlockHeight>,
    pub utxo_validation: bool,
    pub trigger: Trigger,
}

impl TestSetupBuilder {
    pub fn new(seed: u64) -> TestSetupBuilder {
        Self {
            rng: StdRng::seed_from_u64(seed),
            ..Default::default()
        }
    }

    /// setup a contract and add to genesis configuration
    pub fn setup_contract(
        &mut self,
        code: Vec<u8>,
        balances: Option<Vec<(AssetId, u64)>>,
        utxo_id: Option<UtxoId>,
        tx_pointer: Option<TxPointer>,
    ) -> (Salt, ContractId) {
        let contract = Contract::from(code.clone());
        let root = contract.root();
        let salt: Salt = self.rng.gen();
        let contract_id =
            contract.id(&salt.clone(), &root, &Contract::default_state_root());

        self.contracts.insert(
            contract_id,
            ContractConfig {
                contract_id,
                code,
                salt,
                state: None,
                balances,
                tx_id: utxo_id.map(|utxo_id| *utxo_id.tx_id()),
                output_index: utxo_id.map(|utxo_id| utxo_id.output_index()),
                tx_pointer_block_height: tx_pointer.map(|pointer| pointer.block_height()),
                tx_pointer_tx_idx: tx_pointer.map(|pointer| pointer.tx_index()),
            },
        );

        (salt, contract_id)
    }

    /// add input coins from a set of transaction to the genesis config
    pub fn config_coin_inputs_from_transactions(
        &mut self,
        transactions: &[&Script],
    ) -> &mut Self {
        self.initial_coins.extend(
            transactions
                .iter()
                .flat_map(|t| t.inputs())
                .filter_map(|input| {
                    if let Input::CoinSigned(CoinSigned {
                        amount,
                        owner,
                        asset_id,
                        utxo_id,
                        tx_pointer,
                        ..
                    })
                    | Input::CoinPredicate(CoinPredicate {
                        amount,
                        owner,
                        asset_id,
                        utxo_id,
                        tx_pointer,
                        ..
                    }) = input
                    {
                        Some(CoinConfig {
                            tx_id: Some(*utxo_id.tx_id()),
                            output_index: Some(utxo_id.output_index()),
                            tx_pointer_block_height: Some(tx_pointer.block_height()),
                            tx_pointer_tx_idx: Some(tx_pointer.tx_index()),
                            maturity: None,
                            owner: *owner,
                            amount: *amount,
                            asset_id: *asset_id,
                        })
                    } else {
                        None
                    }
                }),
        );

        self
    }

    // setup chainspec and spin up a fuel-node
    pub async fn finalize(&mut self) -> TestContext {
        let mut chain_config = ChainConfig {
            initial_state: Some(StateConfig {
                coins: Some(self.initial_coins.clone()),
                contracts: Some(self.contracts.values().cloned().collect_vec()),
                height: self.starting_block,
                ..StateConfig::default()
            }),
            ..ChainConfig::local_testnet()
        };
        chain_config.consensus_parameters.tx_params.max_gas_per_tx = self.gas_limit;
        chain_config.block_gas_limit = self.gas_limit;
        let config = Config {
            utxo_validation: self.utxo_validation,
            txpool: fuel_core_txpool::Config {
                chain_config: chain_config.clone(),
                min_gas_price: self.min_gas_price,
                ..fuel_core_txpool::Config::default()
            },
            chain_conf: chain_config,
            block_production: self.trigger,
            ..Config::local_node()
        };

        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        TestContext {
            srv,
            rng: self.rng.clone(),
            client,
        }
    }
}

impl Default for TestSetupBuilder {
    fn default() -> Self {
        TestSetupBuilder {
            rng: StdRng::seed_from_u64(2322u64),
            contracts: Default::default(),
            initial_coins: vec![],
            min_gas_price: 0,
            gas_limit: u64::MAX,
            starting_block: None,
            utxo_validation: true,
            trigger: Trigger::Instant,
        }
    }
}
