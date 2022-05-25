// Tests related to the predicate execution feature

use crate::helpers::TestSetupBuilder;
use fuel_crypto::Hasher;
use fuel_tx::{Input, Output, TransactionBuilder};
use fuel_vm::{consts::REG_ONE, prelude::Opcode};
use rand::{rngs::StdRng, Rng, SeedableRng};

#[tokio::test]
async fn transaction_with_predicates_is_rejected_when_feature_disabled() {
    let mut rng = StdRng::seed_from_u64(2322);

    // setup tx with a predicate input
    let asset_id = rng.gen();
    let predicate_tx = TransactionBuilder::script(Default::default(), Default::default())
        .add_input(Input::coin_predicate(
            rng.gen(),
            rng.gen(),
            500,
            asset_id,
            0,
            rng.gen::<[u8; 32]>().to_vec(),
            rng.gen::<[u8; 32]>().to_vec(),
        ))
        .add_output(Output::change(rng.gen(), 0, asset_id))
        .finalize();

    // create test context with predicates disabled
    let context = TestSetupBuilder {
        predicates: false,
        ..Default::default()
    }
    .config_coin_inputs_from_transactions(&[&predicate_tx])
    .finalize()
    .await;

    let result = context.client.submit(&predicate_tx).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn transaction_with_predicates_is_allowed_when_feature_enabled() {
    let mut rng = StdRng::seed_from_u64(2322);

    // setup tx with a predicate input
    let asset_id = rng.gen();
    let predicate = Opcode::RET(REG_ONE).to_bytes().to_vec();
    let owner = (*Hasher::hash(predicate.as_slice())).into();
    let predicate_tx = TransactionBuilder::script(Default::default(), Default::default())
        .add_input(Input::coin_predicate(
            rng.gen(),
            owner,
            500,
            asset_id,
            0,
            // TODO: make this a valid predicate once predicate execution is enabled in the VM.
            predicate,
            vec![],
        ))
        .add_output(Output::change(rng.gen(), 0, asset_id))
        .finalize();

    // create test context with predicates disabled
    let context = TestSetupBuilder {
        predicates: true,
        ..Default::default()
    }
    .config_coin_inputs_from_transactions(&[&predicate_tx])
    .finalize()
    .await;

    let result = context.client.submit(&predicate_tx).await;
    // for now, only ensure no errors occurred
    assert!(result.is_ok());
}
