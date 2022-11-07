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
use fuel_core_interfaces::common::{
    fuel_tx::{
        field::Inputs,
        Contract,
    },
    fuel_vm::prelude::*,
};
use fuel_gql_client::client::FuelClient;
use itertools::Itertools;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::collections::HashMap;

/// Helper for wrapping a currently running node environment
pub struct TestContext {
    pub rng: StdRng,
    pub client: FuelClient,
}

impl TestContext {
    pub async fn new(seed: u64) -> Self {
        let rng = StdRng::seed_from_u64(seed);
        let srv = FuelService::new_node(Config::local_node()).await.unwrap();
        let client = FuelClient::from(srv.bound_address);
        Self { rng, client }
    }
}

/// Helper for configuring the genesis block in tests
pub struct TestSetupBuilder {
    pub rng: StdRng,
    pub contracts: HashMap<ContractId, ContractConfig>,
    pub initial_coins: Vec<CoinConfig>,
    pub min_gas_price: u64,
    pub predicates: bool,
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
    ) -> (Salt, ContractId) {
        let contract = Contract::from(code.clone());
        let root = contract.root();
        let salt: Salt = self.rng.gen();
        let contract_id =
            contract.id(&salt.clone(), &root, &Contract::default_state_root());

        self.contracts.insert(
            contract_id,
            ContractConfig {
                code,
                salt,
                state: None,
                balances,
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
                    if let Input::CoinSigned {
                        amount,
                        owner,
                        asset_id,
                        utxo_id,
                        ..
                    }
                    | Input::CoinPredicate {
                        amount,
                        owner,
                        asset_id,
                        utxo_id,
                        ..
                    } = input
                    {
                        Some(CoinConfig {
                            tx_id: Some(*utxo_id.tx_id()),
                            output_index: Some(utxo_id.output_index() as u64),
                            block_created: None,
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
        let chain_config = ChainConfig {
            initial_state: Some(StateConfig {
                coins: Some(self.initial_coins.clone()),
                contracts: Some(self.contracts.values().cloned().collect_vec()),
                ..StateConfig::default()
            }),
            ..ChainConfig::local_testnet()
        };
        let utxo_validation = true;
        let config = Config {
            utxo_validation,
            txpool: fuel_txpool::Config::new(
                chain_config.clone(),
                self.min_gas_price,
                utxo_validation,
            ),
            chain_conf: chain_config,
            ..Config::local_node()
        };

        let srv = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(srv.bound_address);

        TestContext {
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
            predicates: false,
        }
    }
}
