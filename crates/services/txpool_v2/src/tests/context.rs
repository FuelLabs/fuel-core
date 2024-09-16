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
};
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
    storage::graph::{
        GraphConfig,
        GraphStorage,
    },
    tests::mock_db::{
        MockDBProvider,
        MockDb,
    },
    GasPrice,
};
// TDOO: Reorganize this file

pub(crate) fn create_message_predicate_from_message(
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

pub(crate) fn create_coin_output() -> Output {
    Output::coin(Default::default(), Default::default(), Default::default())
}

pub(crate) fn create_contract_input(
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

pub(crate) fn create_contract_output(contract_id: ContractId) -> Output {
    Output::contract_created(contract_id, Contract::default_state_root())
}

// use some arbitrary large amount, this shouldn't affect the txpool logic except for covering
// the byte and gas price fees.
pub const TEST_COIN_AMOUNT: u64 = 100_000_000u64;

pub(crate) struct PoolContext {
    mock_db: MockDb,
    rng: StdRng,
    pub(crate) config: Config,
}

impl Default for PoolContext {
    fn default() -> Self {
        Self {
            mock_db: MockDb::default(),
            rng: StdRng::seed_from_u64(0),
            config: Default::default(),
        }
    }
}

impl PoolContext {
    pub(crate) fn database_mut(&mut self) -> &mut MockDb {
        &mut self.mock_db
    }

    pub(crate) fn config(self, config: Config) -> Self {
        Self { config, ..self }
    }

    pub(crate) fn build(
        self,
    ) -> Pool<
        MockDBProvider,
        GraphStorage,
        BasicCollisionManager<GraphStorage>,
        RatioTipGasSelection<GraphStorage>,
    > {
        Pool::new(
            MockDBProvider(self.mock_db),
            GraphStorage::new(GraphConfig {
                max_dependent_txn_count: self.config.max_dependent_txn_count,
            }),
            BasicCollisionManager::new(),
            RatioTipGasSelection::new(),
            self.config,
        )
    }

    pub(crate) fn setup_coin(&mut self) -> (Coin, Input) {
        setup_coin(&mut self.rng, Some(&self.mock_db))
    }

    pub(crate) fn create_output_and_input(
        &mut self,
        amount: Word,
    ) -> (Output, UnsetInput) {
        let input = self.random_predicate(AssetId::BASE, amount, None);
        let output = Output::coin(*input.input_owner().unwrap(), amount, AssetId::BASE);
        (output, UnsetInput(input))
    }

    pub(crate) fn random_predicate(
        &mut self,
        asset_id: AssetId,
        amount: Word,
        utxo_id: Option<UtxoId>,
    ) -> Input {
        random_predicate(&mut self.rng, asset_id, amount, utxo_id)
    }

    pub(crate) fn custom_predicate(
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

pub(crate) fn setup_coin(rng: &mut StdRng, mock_db: Option<&MockDb>) -> (Coin, Input) {
    let input = random_predicate(rng, AssetId::BASE, TEST_COIN_AMOUNT, None);
    add_coin_to_state(input, mock_db)
}

pub(crate) fn add_coin_to_state(input: Input, mock_db: Option<&MockDb>) -> (Coin, Input) {
    let mut coin = CompressedCoin::default();
    coin.set_owner(*input.input_owner().unwrap());
    coin.set_amount(TEST_COIN_AMOUNT);
    coin.set_asset_id(*input.asset_id(&AssetId::BASE).unwrap());
    let utxo_id = *input.utxo_id().unwrap();
    if let Some(mock_db) = mock_db {
        mock_db
            .data
            .lock()
            .unwrap()
            .coins
            .insert(utxo_id, coin.clone());
    }
    (coin.uncompress(utxo_id), input)
}

pub(crate) fn random_predicate(
    rng: &mut StdRng,
    asset_id: AssetId,
    amount: Word,
    utxo_id: Option<UtxoId>,
) -> Input {
    // use predicate inputs to avoid expensive cryptography for signatures
    let mut predicate_code: Vec<u8> = vec![op::ret(1)].into_iter().collect();
    // append some randomizing bytes after the predicate has already returned.
    predicate_code.push(rng.gen());
    let owner = Input::predicate_owner(&predicate_code);
    Input::coin_predicate(
        utxo_id.unwrap_or_else(|| rng.gen()),
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

pub struct MockWasmChecker {
    pub result: Result<(), WasmValidityError>,
}

impl WasmChecker for MockWasmChecker {
    fn validate_uploaded_wasm(
        &self,
        _wasm_root: &Bytes32,
    ) -> Result<(), WasmValidityError> {
        self.result
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

#[derive(Debug, Clone)]
pub struct MockTxPoolGasPrice {
    pub gas_price: Option<GasPrice>,
}

impl MockTxPoolGasPrice {
    pub fn new(gas_price: GasPrice) -> Self {
        Self {
            gas_price: Some(gas_price),
        }
    }

    pub fn new_none() -> Self {
        Self { gas_price: None }
    }
}

#[async_trait::async_trait]
impl GasPriceProvider for MockTxPoolGasPrice {
    async fn next_gas_price(&self) -> Result<GasPrice, Error> {
        self.gas_price
            .ok_or(Error::GasPriceNotFound("Gas price not found".to_string()))
    }
}
