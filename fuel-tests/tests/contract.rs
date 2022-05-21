use fuel_core::chain_config::{ChainConfig, CoinConfig, ContractConfig, StateConfig};
use fuel_core::service::{Config, FuelService};
use fuel_crypto::SecretKey;
use fuel_gql_client::client::FuelClient;
use fuel_tx::{Transaction, TransactionBuilder};
use fuel_vm::{consts::*, prelude::*};
use itertools::Itertools;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::collections::HashMap;
use std::io;

struct TestContext {
    rng: StdRng,
    pub client: FuelClient,
}

impl TestContext {
    async fn new(seed: u64) -> Self {
        let rng = StdRng::seed_from_u64(seed);
        let srv = FuelService::new_node(Config::local_node()).await.unwrap();
        let client = FuelClient::from(srv.bound_address);
        Self { rng, client }
    }

    async fn transfer(&mut self, from: Address, to: Address, amount: u64) -> io::Result<Bytes32> {
        let script = Opcode::RET(0x10).to_bytes().to_vec();
        let tx = Transaction::Script {
            gas_price: 0,
            gas_limit: 1_000_000,
            byte_price: 0,
            maturity: 0,
            receipts_root: Default::default(),
            script,
            script_data: vec![],
            inputs: vec![Input::Coin {
                utxo_id: self.rng.gen(),
                owner: from,
                amount,
                asset_id: Default::default(),
                witness_index: 0,
                maturity: 0,
                predicate: vec![],
                predicate_data: vec![],
            }],
            outputs: vec![Output::Coin {
                amount,
                to,
                asset_id: Default::default(),
            }],
            witnesses: vec![vec![].into()],
            metadata: None,
        };
        self.client.submit(&tx).await.map(Into::into)
    }
}

#[tokio::test]
async fn test_contract_salt() {
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);
    let (_, contract_id) = test_builder.setup_contract(vec![]);

    // initialize 10 random transactions that transfer coins and call a contract
    let transactions = (1..=10)
        .into_iter()
        .map(|i| {
            let secret = SecretKey::random(&mut rng);
            TransactionBuilder::script(
                Opcode::RET(REG_ONE).to_bytes().into_iter().collect(),
                vec![],
            )
            .gas_limit(100)
            .gas_price(1)
            .byte_price(1)
            .add_unsigned_coin_input(
                rng.gen(),
                &secret,
                1000 + i,
                Default::default(),
                0,
                vec![],
                vec![],
            )
            .add_input(Input::Contract {
                utxo_id: Default::default(),
                balance_root: Default::default(),
                state_root: Default::default(),
                contract_id,
            })
            .add_output(Output::Change {
                amount: 0,
                asset_id: Default::default(),
                to: rng.gen(),
            })
            .add_output(Output::Contract {
                input_index: 1,
                balance_root: Default::default(),
                state_root: Default::default(),
            })
            .finalize()
        })
        .collect_vec();

    // setup genesis block with coins that transactions can spend
    test_builder.config_coin_inputs_from_transactions(&transactions);

    // spin up node
    let TestContext { client, .. } = test_builder.finalize().await;

    let contract = client
        .contract(format!("{:#x}", contract_id).as_str())
        .await
        .unwrap();

    // Check that salt is 0x Hex prefixed
    let salt = contract.unwrap().salt;
    assert_eq!("0x", &salt.to_string()[..2]);
}

#[tokio::test]
async fn test_contract_balance_default() {
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);
    let (_, contract_id) = test_builder.setup_contract(vec![]);

    // initialize 10 random transactions that transfer coins and call a contract
    let transactions = (1..=10)
        .into_iter()
        .map(|i| {
            let secret = SecretKey::random(&mut rng);
            TransactionBuilder::script(
                Opcode::RET(REG_ONE).to_bytes().into_iter().collect(),
                vec![],
            )
            .gas_limit(100)
            .gas_price(1)
            .byte_price(1)
            .add_unsigned_coin_input(
                rng.gen(),
                &secret,
                1000 + i,
                Default::default(),
                0,
                vec![],
                vec![],
            )
            .add_input(Input::Contract {
                utxo_id: Default::default(),
                balance_root: Default::default(),
                state_root: Default::default(),
                contract_id,
            })
            .add_output(Output::Change {
                amount: 0,
                asset_id: Default::default(),
                to: rng.gen(),
            })
            .add_output(Output::Contract {
                input_index: 1,
                balance_root: Default::default(),
                state_root: Default::default(),
            })
            .finalize()
        })
        .collect_vec();

    // setup genesis block with coins that transactions can spend
    test_builder.config_coin_inputs_from_transactions(&transactions);

    // spin up node
    let TestContext { client, .. } = test_builder.finalize().await;

    let asset_id = AssetId::new([1u8; 32]);

    let balance = client
        .contract_balance(
            format!("{:#x}", contract_id).as_str(),
            Some(format!("{:#x}", asset_id).as_str()),
        )
        .await
        .unwrap();

    assert_eq!(balance, 0); // Ensures default value is working properly
}

#[tokio::test]
async fn test_contract_balance() {
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);
    let (_, contract_id) = test_builder.setup_funded_contract(vec![]);

    // initialize 10 random transactions that transfer coins and call a contract
    let transactions = (1..=10)
        .into_iter()
        .map(|i| {
            let secret = SecretKey::random(&mut rng);
            TransactionBuilder::script(
                Opcode::RET(REG_ONE).to_bytes().into_iter().collect(),
                vec![],
            )
            .gas_limit(100)
            .gas_price(1)
            .byte_price(1)
            .add_unsigned_coin_input(
                rng.gen(),
                &secret,
                1000 + i,
                Default::default(),
                0,
                vec![],
                vec![],
            )
            .add_input(Input::Contract {
                utxo_id: Default::default(),
                balance_root: Default::default(),
                state_root: Default::default(),
                contract_id,
            })
            .add_output(Output::Change {
                amount: 0,
                asset_id: Default::default(),
                to: rng.gen(),
            })
            .add_output(Output::Contract {
                input_index: 1,
                balance_root: Default::default(),
                state_root: Default::default(),
            })
            .finalize()
        })
        .collect_vec();

    // setup genesis block with coins that transactions can spend
    test_builder.config_coin_inputs_from_transactions(&transactions);

    // spin up node
    let TestContext { client, .. } = test_builder.finalize().await;

    let asset_id = AssetId::new([1u8; 32]);

    let balance = client
        .contract_balance(
            format!("{:#x}", contract_id).as_str(),
            Some(format!("{:#x}", asset_id).as_str()),
        )
        .await
        .unwrap();

    assert_eq!(balance, 500);
}
/// Helper for configuring the genesis block in tests
struct TestSetupBuilder {
    rng: StdRng,
    contracts: HashMap<ContractId, ContractConfig>,
    initial_coins: Vec<CoinConfig>,
    min_gas_price: u64,
    min_byte_price: u64,
}

impl TestSetupBuilder {
    pub fn new(seed: u64) -> TestSetupBuilder {
        Self {
            rng: StdRng::seed_from_u64(seed),
            ..Default::default()
        }
    }

    /// setup a contract and add to genesis configuration
    fn setup_contract(&mut self, code: Vec<u8>) -> (Salt, ContractId) {
        let contract = Contract::from(code.clone());
        let root = contract.root();
        let salt: Salt = self.rng.gen();
        let contract_id = contract.id(&salt.clone(), &root, &Contract::default_state_root());

        self.contracts.insert(
            contract_id,
            ContractConfig {
                code,
                salt,
                state: None,
                balances: None,
            },
        );

        (salt, contract_id)
    }

    /// setup a contract and add to genesis configuration
    fn setup_funded_contract(&mut self, code: Vec<u8>) -> (Salt, ContractId) {
        let contract = Contract::from(code.clone());
        let root = contract.root();
        let salt: Salt = self.rng.gen();
        let contract_id = contract.id(&salt.clone(), &root, &Contract::default_state_root());

        let test_asset_id: AssetId = AssetId::new([1u8; 32]);
        let test_balance: u64 = 500;
        let balances = vec![(test_asset_id, test_balance)];

        self.contracts.insert(
            contract_id,
            ContractConfig {
                code,
                salt,
                state: None,
                balances: Some(balances),
            },
        );

        (salt, contract_id)
    }

    /// add input coins from a set of transaction to the genesis config
    fn config_coin_inputs_from_transactions(&mut self, transactions: &[Transaction]) -> &mut Self {
        self.initial_coins
            .extend(
                transactions
                    .iter()
                    .flat_map(|t| t.inputs())
                    .filter_map(|input| {
                        if let Input::Coin {
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
        let config = Config {
            utxo_validation: true,
            tx_pool_config: fuel_txpool::Config {
                min_byte_price: self.min_byte_price,
                min_gas_price: self.min_gas_price,
                ..Default::default()
            },
            chain_conf: ChainConfig {
                initial_state: Some(StateConfig {
                    coins: Some(self.initial_coins.clone()),
                    contracts: Some(self.contracts.values().cloned().collect_vec()),
                    ..StateConfig::default()
                }),
                ..ChainConfig::local_testnet()
            },
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
            min_byte_price: 0,
        }
    }
}
