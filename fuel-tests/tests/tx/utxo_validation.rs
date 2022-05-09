// Tests involving utxo-validation enabled

use crate::tx::TestContext;
use fuel_core::chain_config::{ChainConfig, CoinConfig, ContractConfig, StateConfig};
use fuel_core::service::{Config, FuelService};
use fuel_crypto::SecretKey;
use fuel_gql_client::client::types::TransactionStatus;
use fuel_gql_client::client::FuelClient;
use fuel_tx::{Transaction, TransactionBuilder};
use fuel_vm::{consts::*, prelude::*};
use itertools::Itertools;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::collections::HashMap;

#[tokio::test]
async fn submit_utxo_verified_tx_with_min_gas_price() {
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

    // submit transactions and verify their status
    for tx in transactions {
        let id = client.submit(&tx).await.unwrap();
        // verify that the tx returned from the api matches the submitted tx
        let ret_tx = client
            .transaction(&id.0.to_string())
            .await
            .unwrap()
            .unwrap()
            .transaction;

        let transaction_result = client
            .transaction_status(&ret_tx.id().to_string())
            .await
            .ok()
            .unwrap();

        if let TransactionStatus::Success { block_id, .. } = transaction_result.clone() {
            let block_exists = client.block(&block_id).await.unwrap();

            assert!(block_exists.is_some());
        }

        // Once https://github.com/FuelLabs/fuel-core/issues/50 is resolved this should rely on the Submitted Status rather than Success
        assert!(matches!(
            transaction_result,
            TransactionStatus::Success { .. }
        ));
    }
}

#[tokio::test]
async fn submit_utxo_verified_tx_below_min_gas_price_fails() {
    // initialize transaction
    let tx = TransactionBuilder::script(
        Opcode::RET(REG_ONE).to_bytes().into_iter().collect(),
        vec![],
    )
    .gas_limit(100)
    .gas_price(1)
    .byte_price(1)
    .finalize();

    // initialize node with higher minimum gas price
    let mut test_builder = TestSetupBuilder::new(2322u64);
    test_builder.min_byte_price = 10;
    test_builder.min_gas_price = 10;
    let TestContext { client, .. } = test_builder.finalize().await;

    let result = client.submit(&tx).await;
    assert!(result.is_err());
    assert!(result
        .err()
        .unwrap()
        .to_string()
        .contains("The gas price is too low"));
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
    pub async fn finalize(self) -> TestContext {
        let config = Config {
            utxo_validation: true,
            tx_pool_config: fuel_txpool::Config {
                min_byte_price: self.min_byte_price,
                min_gas_price: self.min_gas_price,
                ..Default::default()
            },
            chain_conf: ChainConfig {
                initial_state: Some(StateConfig {
                    coins: Some(self.initial_coins),
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
            rng: self.rng,
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
