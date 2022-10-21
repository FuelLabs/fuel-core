use crate::MockDb;
use fuel_core_interfaces::{
    common::{
        fuel_asm::Opcode,
        fuel_crypto::rand::{
            rngs::StdRng,
            Rng,
        },
        fuel_tx::{
            Output,
            TransactionBuilder,
            UtxoId,
        },
        prelude::{
            AssetId,
            Input,
            Transaction,
            Word,
        },
    },
    model::{
        Coin,
        CoinStatus,
    },
};

// use some arbitrary large amount, this shouldn't affect the txpool logic except for covering
// the byte and gas price fees.
pub const TEST_COIN_AMOUNT: u64 = 100_000_000u64;

#[allow(dead_code)]
// create a randomly generated unique transaction that sets up the mockdb for validity
pub(crate) fn setup_tx(
    gas_price: Word,
    rng: &mut StdRng,
    script: bool,
    mock_db: Option<&MockDb>,
    utxo_id: Option<UtxoId>,
    outputs: Option<Vec<Output>>,
) -> Transaction {
    let (_, input) = setup_coin(rng, mock_db);
    tx_from_input(gas_price, input, script, outputs)
}

pub(crate) fn tx_from_input(
    gas_price: Word,
    input: Input,
    script: bool,
    outputs: Option<Vec<Output>>,
) -> Transaction {
    let mut builder = if script {
        TransactionBuilder::script(vec![], vec![])
    } else {
        TransactionBuilder::create(
            Default::default(),
            Default::default(),
            Default::default(),
        )
    };

    builder.add_input(input);
    builder.gas_price(gas_price);
    if let Some(outputs) = outputs {
        for output in outputs {
            builder.add_output(output);
        }
    }
    builder.finalize()
}

pub(crate) fn setup_coin(rng: &mut StdRng, mock_db: Option<&MockDb>) -> (Coin, Input) {
    let input = random_predicate(rng, AssetId::BASE, TEST_COIN_AMOUNT, None);
    let coin = Coin {
        owner: input.input_owner().unwrap().clone(),
        amount: TEST_COIN_AMOUNT,
        asset_id: input.asset_id().unwrap().clone(),
        maturity: Default::default(),
        status: CoinStatus::Unspent,
        block_created: Default::default(),
    };
    if let Some(mock_db) = mock_db {
        mock_db
            .data
            .lock()
            .unwrap()
            .coins
            .insert(input.utxo_id().unwrap().clone(), coin.clone());
    }
    (coin, input)
}

pub(crate) fn create_output_and_input(
    rng: &mut StdRng,
    amount: Word,
) -> (Output, UnsetInput) {
    let input = random_predicate(rng, AssetId::BASE, amount, None);
    let output =
        Output::coin(input.input_owner().unwrap().clone(), amount, AssetId::BASE);
    (output, UnsetInput(input))
}

pub struct UnsetInput(Input);

impl UnsetInput {
    pub fn into_input(self, new_utxo_id: UtxoId) -> Input {
        let mut input = self.0;
        match &mut input {
            Input::CoinSigned { utxo_id, .. }
            | Input::CoinPredicate { utxo_id, .. }
            | Input::Contract { utxo_id, .. } => {
                *utxo_id = new_utxo_id;
            }
            Input::MessageSigned { .. } => {}
            Input::MessagePredicate { .. } => {}
        }
        input
    }
}

pub(crate) fn random_predicate(
    rng: &mut StdRng,
    asset_id: AssetId,
    amount: Word,
    utxo_id: Option<UtxoId>,
) -> Input {
    // use predicate inputs to avoid expensive cryptography for signatures
    let mut predicate_code: Vec<u8> = vec![Opcode::RET(1)].into_iter().collect();
    // append some randomizing bytes after the predicate has already returned.
    predicate_code.push(rng.gen());
    let owner = Input::predicate_owner(&predicate_code);
    Input::coin_predicate(
        utxo_id.unwrap_or(rng.gen()),
        owner,
        amount,
        asset_id,
        Default::default(),
        0,
        predicate_code,
        vec![],
    )
}
