use std::{
    collections::HashSet,
    sync::Arc,
};

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
        BlockHeight,
        ChainId,
        Word,
    },
    fuel_vm::{
        checked_transaction::EstimatePredicates,
        interpreter::MemoryInstance,
        predicate::EmptyStorage,
    },
    services::txpool::ArcPoolTx,
};
use parking_lot::RwLock;
use tokio::sync::broadcast::Receiver;

use crate::{
    collision_manager::basic::BasicCollisionManager,
    config::Config,
    error::Error,
    new_service,
    pool::{
        Pool,
        TxPoolStats,
    },
    selection_algorithms::ratio_tip_gas::RatioTipGasSelection,
    service::{
        memory::MemoryPool,
        verifications::Verification,
        Shared,
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
    GasPrice,
    Service,
};

use super::mocks::{
    MockConsensusParametersProvider,
    MockImporter,
    MockP2P,
    MockTxPoolGasPrice,
    MockWasmChecker,
};

// use some arbitrary large amount, this shouldn't affect the txpool logic except for covering
// the byte and gas price fees.
pub const TEST_COIN_AMOUNT: u64 = 100_000_000u64;
pub const GAS_LIMIT: Word = 100000;
pub const DEFAULT_EXPIRATION_HEIGHT: BlockHeight = BlockHeight::new(1000);

pub struct TestPoolUniverse {
    mock_db: MockDb,
    rng: StdRng,
    pub config: Config,
    pool: Option<Shared<TxPool>>,
    stats_receiver: Option<tokio::sync::watch::Receiver<TxPoolStats>>,
}

impl Default for TestPoolUniverse {
    fn default() -> Self {
        Self {
            mock_db: MockDb::default(),
            rng: StdRng::seed_from_u64(0),
            config: Default::default(),
            pool: None,
            stats_receiver: None,
        }
    }
}

impl TestPoolUniverse {
    pub fn database_mut(&mut self) -> &mut MockDb {
        &mut self.mock_db
    }

    pub fn database(&self) -> &MockDb {
        &self.mock_db
    }

    pub fn latest_stats(&self) -> TxPoolStats {
        if let Some(receiver) = &self.stats_receiver {
            *receiver.borrow()
        } else {
            TxPoolStats::default()
        }
    }

    pub fn config(self, config: Config) -> Self {
        if self.pool.is_some() {
            panic!("Pool already built");
        }
        Self { config, ..self }
    }

    pub fn build_pool(&mut self) {
        let (tx, rx) = tokio::sync::watch::channel(TxPoolStats::default());
        let pool = Arc::new(RwLock::new(Pool::new(
            GraphStorage::new(GraphConfig {
                max_txs_chain_count: self.config.max_txs_chain_count,
            }),
            BasicCollisionManager::new(),
            RatioTipGasSelection::new(),
            self.config.clone(),
            tx,
        )));
        self.stats_receiver = Some(rx);
        self.pool = Some(pool.clone());
    }

    pub fn build_service(
        &self,
        p2p: Option<MockP2P>,
        importer: Option<MockImporter>,
    ) -> Service<MockDb> {
        let gas_price = 0;
        let mut p2p = p2p.unwrap_or_else(|| MockP2P::new_with_txs(vec![]));
        // set default handlers for p2p methods after test is set up, so they will be last on the FIFO
        // ordering of methods handlers: https://docs.rs/mockall/0.12.1/mockall/index.html#matching-multiple-calls
        p2p.expect_notify_gossip_transaction_validity()
            .returning(move |_, _| Ok(()));
        p2p.expect_broadcast_transaction()
            .returning(move |_| Ok(()));
        p2p.expect_subscribe_new_peers()
            .returning(|| Box::pin(fuel_core_services::stream::pending()));

        let importer = importer.unwrap_or_else(|| MockImporter::with_blocks(vec![]));
        let gas_price_provider = MockTxPoolGasPrice::new(gas_price);
        let mut consensus_parameters_provider =
            MockConsensusParametersProvider::default();
        consensus_parameters_provider
            .expect_latest_consensus_parameters()
            .returning(|| (0, Arc::new(Default::default())));

        new_service(
            ChainId::default(),
            self.config.clone(),
            p2p,
            importer,
            MockDBProvider(self.mock_db.clone()),
            consensus_parameters_provider,
            Default::default(),
            gas_price_provider,
            MockWasmChecker { result: Ok(()) },
        )
    }

    pub fn build_script_transaction(
        &mut self,
        inputs: Option<Vec<Input>>,
        outputs: Option<Vec<Output>>,
        tip: u64,
    ) -> Transaction {
        let mut inputs = inputs.unwrap_or_default();
        let (_, gas_coin) = self.setup_coin();
        inputs.push(gas_coin);
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
        tx_builder.expiration(DEFAULT_EXPIRATION_HEIGHT);
        tx_builder.finalize().into()
    }

    // Returns the added transaction and the list of transactions that were removed from the pool
    pub fn verify_and_insert(
        &mut self,
        tx: Transaction,
    ) -> Result<(ArcPoolTx, Vec<ArcPoolTx>), Error> {
        if let Some(pool) = &self.pool {
            let mut mock_consensus_params_provider =
                MockConsensusParametersProvider::default();
            mock_consensus_params_provider
                .expect_latest_consensus_parameters()
                .returning(|| (0, Arc::new(ConsensusParameters::standard())));
            let verification = Verification {
                persistent_storage_provider: Arc::new(MockDBProvider(
                    self.mock_db.clone(),
                )),
                gas_price_provider: Arc::new(MockTxPoolGasPrice::new(0)),
                consensus_parameters_provider: Arc::new(mock_consensus_params_provider),
                wasm_checker: Arc::new(MockWasmChecker::new(Ok(()))),
                memory_pool: MemoryPool::new(),
            };
            let tx = verification.perform_all_verifications(
                tx,
                &pool.clone(),
                Default::default(),
                true,
            )?;
            let tx = Arc::new(tx);
            Ok((tx.clone(), pool.write().insert(tx, &self.mock_db)?))
        } else {
            panic!("Pool needs to be built first");
        }
    }

    pub fn verify_and_insert_with_gas_price(
        &mut self,
        tx: Transaction,
        gas_price: GasPrice,
    ) -> Result<Vec<ArcPoolTx>, Error> {
        if let Some(pool) = &self.pool {
            let mut mock_consensus_params_provider =
                MockConsensusParametersProvider::default();
            mock_consensus_params_provider
                .expect_latest_consensus_parameters()
                .returning(|| (0, Arc::new(ConsensusParameters::standard())));
            let verification = Verification {
                persistent_storage_provider: Arc::new(MockDBProvider(
                    self.mock_db.clone(),
                )),
                gas_price_provider: Arc::new(MockTxPoolGasPrice::new(gas_price)),
                consensus_parameters_provider: Arc::new(mock_consensus_params_provider),
                wasm_checker: Arc::new(MockWasmChecker::new(Ok(()))),
                memory_pool: MemoryPool::new(),
            };
            let tx = verification.perform_all_verifications(
                tx,
                &pool.clone(),
                Default::default(),
                true,
            )?;
            pool.write().insert(Arc::new(tx), &self.mock_db)
        } else {
            panic!("Pool needs to be built first");
        }
    }

    pub fn verify_and_insert_with_consensus_params_wasm_checker(
        &mut self,
        tx: Transaction,
        consensus_params: ConsensusParameters,
        wasm_checker: MockWasmChecker,
    ) -> Result<Vec<ArcPoolTx>, Error> {
        if let Some(pool) = &self.pool {
            let mut mock_consensus_params_provider =
                MockConsensusParametersProvider::default();
            mock_consensus_params_provider
                .expect_latest_consensus_parameters()
                .returning(move || (0, Arc::new(consensus_params.clone())));
            let verification = Verification {
                persistent_storage_provider: Arc::new(MockDBProvider(
                    self.mock_db.clone(),
                )),
                gas_price_provider: Arc::new(MockTxPoolGasPrice::new(0)),
                consensus_parameters_provider: Arc::new(mock_consensus_params_provider),
                wasm_checker: Arc::new(wasm_checker),
                memory_pool: MemoryPool::new(),
            };
            let tx = verification.perform_all_verifications(
                tx,
                &pool.clone(),
                Default::default(),
                true,
            )?;
            pool.write().insert(Arc::new(tx), &self.mock_db)
        } else {
            panic!("Pool needs to be built first");
        }
    }

    pub fn assert_pool_integrity(&self, expected_txs: &[ArcPoolTx]) {
        let stats = self.latest_stats();
        assert_eq!(stats.tx_count, expected_txs.len() as u64);
        let mut total_gas: u64 = 0;
        let mut total_size: u64 = 0;
        for tx in expected_txs {
            total_gas = total_gas.checked_add(tx.max_gas()).unwrap();
            total_size = total_size
                .checked_add(tx.metered_bytes_size() as u64)
                .unwrap();
        }
        assert_eq!(stats.total_gas, total_gas);
        assert_eq!(stats.total_size, total_size);
        let pool = self.pool.as_ref().unwrap();
        let pool = pool.read();
        let storage_ids_dependencies = pool.storage.assert_integrity(expected_txs);
        let txs_without_dependencies = expected_txs
            .iter()
            .zip(storage_ids_dependencies)
            .filter_map(|(tx, (_, has_dependencies))| {
                if !has_dependencies {
                    Some(tx.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        pool.selection_algorithm
            .assert_integrity(&txs_without_dependencies);
        pool.collision_manager.assert_integrity(expected_txs);
        let txs: HashSet<TxId> = expected_txs.iter().map(|tx| tx.id()).collect();
        pool.assert_integrity(txs);
    }

    pub fn get_pool(&self) -> Shared<TxPool> {
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

    pub fn create_output_and_input(&mut self) -> (Output, UnsetInput) {
        let input = self.random_predicate(AssetId::BASE, 1, None);
        let output = Output::coin(*input.input_owner().unwrap(), 1, AssetId::BASE);
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

    pub async fn waiting_txs_insertion(
        &self,
        mut new_tx_notification_subscribe: Receiver<Bytes32>,
        mut tx_ids: Vec<TxId>,
    ) {
        while !tx_ids.is_empty() {
            let tx_id = new_tx_notification_subscribe.recv().await.unwrap();
            let index = tx_ids.iter().position(|id| *id == tx_id).unwrap();
            tx_ids.remove(index);
        }
    }
}

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
        let _ =
            tx.estimate_predicates(&params.into(), MemoryInstance::new(), &EmptyStorage);
        tx.inputs()[0].clone()
    }
}
