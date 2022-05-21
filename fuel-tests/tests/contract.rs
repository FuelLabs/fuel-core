use fuel_core::chain_config::{ChainConfig, CoinConfig, ContractConfig, StateConfig};
use fuel_core::service::{Config, FuelService};
use crate::common::TestSetupBuilder;
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