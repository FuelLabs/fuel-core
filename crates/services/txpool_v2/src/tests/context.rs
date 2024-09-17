use std::sync::Arc;

use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    entities::{
        coins::coin::{
            Coin,
            CompressedCoin,
        },
        relayer::message::{
            Message,
            MessageV1,
        },
    },
    fuel_asm::op,
    fuel_crypto::rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    },
    fuel_tx::{
        field::Inputs,
        input::{
            coin::{
                CoinPredicate,
                CoinSigned,
            },
            contract::Contract as ContractInput,
            Input,
        },
        Bytes32,
        ConsensusParameters,
        Contract,
        ContractId,
        Finalizable,
        Output,
        Transaction,
        TransactionBuilder,
        TxId,
        UtxoId,
    },
    fuel_types::{
        AssetId,
        Word,
    },
    fuel_vm::{
        checked_transaction::{
            Checked,
            EstimatePredicates,
        },
        interpreter::MemoryInstance,
    },
    services::txpool::PoolTransaction,
};
use parking_lot::RwLock;
use petgraph::graph::NodeIndex;

use crate::{
    collision_manager::basic::BasicCollisionManager,
    config::Config,
    error::Error,
    pool::Pool,
    ports::{
        GasPriceProvider,
        WasmChecker,
        WasmValidityError,
    },
    selection_algorithms::ratio_tip_gas::RatioTipGasSelection,
    service::{
        RemovedTransactions,
        TxPool,
    },
    storage::graph::{
        GraphConfig,
        GraphStorage,
    },
    tests::mocks::{
        MockDBProvider,
        MockDb,
    },
    verifications::perform_all_verifications,
    GasPrice,
};

use super::mocks::{
    MockTxPoolGasPrice,
    MockWasmChecker,
};
// TDOO: Reorganize this file

pub fn create_message_predicate_from_message(
    amount: Word,
    nonce: u64,
) -> (Message, Input) {
    let predicate = vec![op::ret(1)].into_iter().collect::<Vec<u8>>();
    let message = MessageV1 {
        sender: Default::default(),
        recipient: Input::predicate_owner(&predicate),
        nonce: nonce.into(),
        amount,
        data: vec![],
        da_height: Default::default(),
    };

    (
        message.clone().into(),
        Input::message_coin_predicate(
            message.sender,
            Input::predicate_owner(&predicate),
            message.amount,
            message.nonce,
            Default::default(),
            predicate,
            Default::default(),
        )
        .into_default_estimated(),
    )
}

pub fn create_coin_output() -> Output {
    Output::coin(Default::default(), Default::default(), Default::default())
}

pub fn create_contract_input(
    tx_id: TxId,
    output_index: u16,
    contract_id: ContractId,
) -> Input {
    Input::contract(
        UtxoId::new(tx_id, output_index),
        Default::default(),
        Default::default(),
        Default::default(),
        contract_id,
    )
}

pub fn create_contract_output(contract_id: ContractId) -> Output {
    Output::contract_created(contract_id, Contract::default_state_root())
}

// use some arbitrary large amount, this shouldn't affect the txpool logic except for covering
// the byte and gas price fees.
pub const TEST_COIN_AMOUNT: u64 = 100_000_000u64;
pub const GAS_LIMIT: Word = 100000;

pub struct TestPoolUniverse {
    mock_db: MockDb,
    rng: StdRng,
    pub config: Config,
    pool: Option<TxPool<MockDBProvider>>,
}

impl Default for TestPoolUniverse {
    fn default() -> Self {
        Self {
            mock_db: MockDb::default(),
            rng: StdRng::seed_from_u64(0),
            config: Default::default(),
            pool: None,
        }
    }
}

impl TestPoolUniverse {
    pub fn database_mut(&mut self) -> &mut MockDb {
        &mut self.mock_db
    }

    pub fn config(self, config: Config) -> Self {
        if self.pool.is_some() {
            panic!("Pool already built");
        }
        Self { config, ..self }
    }

    pub fn build_pool(&mut self) -> TxPool<MockDBProvider> {
        let pool = Arc::new(RwLock::new(Pool::new(
            MockDBProvider(self.mock_db.clone()),
            GraphStorage::new(GraphConfig {
                max_dependent_txn_count: self.config.max_dependent_txn_count,
            }),
            BasicCollisionManager::new(),
            RatioTipGasSelection::new(),
            self.config.clone(),
        )));
        self.pool = Some(pool.clone());
        pool
    }

    pub fn build_script_transaction(
        &mut self,
        inputs: Option<Vec<Input>>,
        outputs: Option<Vec<Output>>,
        tip: u64,
    ) -> Transaction {
        let mut inputs = inputs.unwrap_or_default();
        let (_, gas_coin) = self.setup_coin();
        inputs.push(gas_coin.into());
        let outputs = outputs.unwrap_or_default();
        let mut tx_builder = TransactionBuilder::script(vec![], vec![]);
        tx_builder.script_gas_limit(GAS_LIMIT);
        for input in inputs {
            tx_builder.add_input(input);
        }
        for output in outputs {
            tx_builder.add_output(output);
        }
        tx_builder.tip(tip);
        tx_builder.max_fee_limit(10000);
        tx_builder.finalize().into()
    }

    pub async fn verify_and_insert(
        &mut self,
        tx: Transaction,
    ) -> Result<RemovedTransactions, Error> {
        if let Some(pool) = &self.pool {
            let tx = perform_all_verifications(
                tx,
                pool.clone(),
                Default::default(),
                &ConsensusParameters::default(),
                0,
                &MockTxPoolGasPrice::new(0),
                &MockWasmChecker::new(Ok(())),
                MemoryInstance::new(),
            )
            .await?;
            pool.write().insert(tx)
        } else {
            panic!("Pool needs to be built first");
        }
    }

    pub async fn verify_and_insert_with_gas_price(
        &mut self,
        tx: Transaction,
        gas_price: GasPrice,
    ) -> Result<RemovedTransactions, Error> {
        if let Some(pool) = &self.pool {
            let tx = perform_all_verifications(
                tx,
                pool.clone(),
                Default::default(),
                &ConsensusParameters::default(),
                0,
                &MockTxPoolGasPrice::new(gas_price),
                &MockWasmChecker::new(Ok(())),
                MemoryInstance::new(),
            )
            .await?;
            pool.write().insert(tx)
        } else {
            panic!("Pool needs to be built first");
        }
    }

    pub async fn verify_and_insert_with_consensus_params_wasm_checker(
        &mut self,
        tx: Transaction,
        consensus_params: ConsensusParameters,
        wasm_checker: MockWasmChecker,
    ) -> Result<RemovedTransactions, Error> {
        if let Some(pool) = &self.pool {
            let tx = perform_all_verifications(
                tx,
                pool.clone(),
                Default::default(),
                &consensus_params,
                0,
                &MockTxPoolGasPrice::new(0),
                &wasm_checker,
                MemoryInstance::new(),
            )
            .await?;
            pool.write().insert(tx)
        } else {
            panic!("Pool needs to be built first");
        }
    }

    pub fn get_pool(&self) -> TxPool<MockDBProvider> {
        self.pool.clone().unwrap()
    }

    pub fn setup_coin(&mut self) -> (Coin, Input) {
        let input = self.random_predicate(AssetId::BASE, TEST_COIN_AMOUNT, None);

        // add coin to the state
        let mut coin = CompressedCoin::default();
        coin.set_owner(*input.input_owner().unwrap());
        coin.set_amount(TEST_COIN_AMOUNT);
        coin.set_asset_id(*input.asset_id(&AssetId::BASE).unwrap());
        let utxo_id = *input.utxo_id().unwrap();
        self.mock_db
            .data
            .lock()
            .unwrap()
            .coins
            .insert(utxo_id, coin.clone());
        (coin.uncompress(utxo_id), input)
    }

    // TODO: Change this
    pub fn create_output_and_input(&mut self, amount: Word) -> (Output, UnsetInput) {
        let input = self.random_predicate(AssetId::BASE, amount, None);
        let output = Output::coin(*input.input_owner().unwrap(), amount, AssetId::BASE);
        (output, UnsetInput(input))
    }

    pub fn random_predicate(
        &mut self,
        asset_id: AssetId,
        amount: Word,
        utxo_id: Option<UtxoId>,
    ) -> Input {
        // use predicate inputs to avoid expensive cryptography for signatures
        let mut predicate_code: Vec<u8> = vec![op::ret(1)].into_iter().collect();
        // append some randomizing bytes after the predicate has already returned.
        predicate_code.push(self.rng.gen());
        let owner = Input::predicate_owner(&predicate_code);
        Input::coin_predicate(
            utxo_id.unwrap_or_else(|| self.rng.gen()),
            owner,
            amount,
            asset_id,
            Default::default(),
            Default::default(),
            predicate_code,
            vec![],
        )
        .into_default_estimated()
    }

    pub fn custom_predicate(
        &mut self,
        asset_id: AssetId,
        amount: Word,
        code: Vec<u8>,
        utxo_id: Option<UtxoId>,
    ) -> Input {
        let owner = Input::predicate_owner(&code);
        Input::coin_predicate(
            utxo_id.unwrap_or_else(|| self.rng.gen()),
            owner,
            amount,
            asset_id,
            Default::default(),
            Default::default(),
            code,
            vec![],
        )
    }
}

pub struct UnsetInput(Input);

impl UnsetInput {
    pub fn into_input(self, new_utxo_id: UtxoId) -> Input {
        let mut input = self.0;
        match &mut input {
            Input::CoinSigned(CoinSigned { utxo_id, .. })
            | Input::CoinPredicate(CoinPredicate { utxo_id, .. })
            | Input::Contract(ContractInput { utxo_id, .. }) => {
                *utxo_id = new_utxo_id;
            }
            _ => {}
        }
        input
    }
}

pub trait IntoEstimated {
    #[cfg(test)]
    fn into_default_estimated(self) -> Self;
    fn into_estimated(self, params: &ConsensusParameters) -> Self;
}

impl IntoEstimated for Input {
    #[cfg(test)]
    fn into_default_estimated(self) -> Self {
        self.into_estimated(&Default::default())
    }

    fn into_estimated(self, params: &ConsensusParameters) -> Self {
        let mut tx = TransactionBuilder::script(vec![], vec![])
            .add_input(self)
            .finalize();
        let _ = tx.estimate_predicates(&params.into(), MemoryInstance::new());
        tx.inputs()[0].clone()
    }
}
