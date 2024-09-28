#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::arithmetic_side_effects)]

use crate::{
    test_helpers::TextContext,
    types::TxId,
    Config,
};
use fuel_core_storage::rand::{
    prelude::SliceRandom,
    rngs::StdRng,
    thread_rng,
    Rng,
    RngCore,
    SeedableRng,
};
use fuel_core_types::{
    fuel_tx::{
        field::Tip,
        ConsensusParameters,
        Finalizable,
        GasCosts,
        Input,
        Output,
        Transaction,
        TransactionBuilder,
        UtxoId,
    },
    fuel_types::AssetId,
    fuel_vm::checked_transaction::{
        Checked,
        IntoChecked,
    },
    services::txpool::Metadata,
};

#[derive(Debug, Clone, Copy)]
struct Limits {
    max_inputs: usize,
    min_outputs: u16,
    max_outputs: u16,
    utxo_id_range: u8,
    gas_limit_range: u64,
}

fn some_transaction(
    limits: Limits,
    tip: u64,
    rng: &mut StdRng,
) -> (Checked<Transaction>, Metadata) {
    const AMOUNT: u64 = 10000;

    let mut consensus_parameters = ConsensusParameters::standard();
    consensus_parameters.set_gas_costs(GasCosts::free());

    let mut builder = TransactionBuilder::script(vec![], vec![]);
    builder.with_params(consensus_parameters.clone());

    let mut owner = [0u8; 32];
    owner[0] = 123;
    owner[1] = 222;
    let owner = owner.into();

    let mut random_ids = (0..limits.utxo_id_range)
        .map(|_| rng.gen_range(0..limits.utxo_id_range))
        .collect::<Vec<_>>();
    random_ids.sort();
    random_ids.dedup();
    random_ids.shuffle(rng);

    let tx_id_byte = random_ids.pop().expect("No random ids");
    let tx_id: TxId = [tx_id_byte; 32].into();
    let max_gas = rng.gen_range(0..limits.gas_limit_range) + 1;

    let inputs_count = limits.max_inputs.min(random_ids.len());
    let inputs_count = rng.gen_range(1..inputs_count);
    let inputs = random_ids.into_iter().take(inputs_count);

    for input_utxo_id in inputs {
        let output_index = rng.gen_range(0..limits.max_outputs);
        let utxo_id = UtxoId::new([input_utxo_id; 32].into(), output_index);

        builder.add_input(Input::coin_signed(
            utxo_id,
            owner,
            AMOUNT,
            AssetId::BASE,
            Default::default(),
            Default::default(),
        ));
    }

    // We can't have more outputs than inputs.
    let outputs = rng
        .gen_range(limits.min_outputs..limits.max_outputs)
        .min(inputs_count as u16);

    for _ in 0..outputs {
        let output = Output::coin(owner, AMOUNT, AssetId::BASE);
        builder.add_output(output);
    }

    builder.add_witness(Default::default());

    let mut tx = builder.finalize();
    tx.set_tip(tip);
    let tx = Transaction::from(tx);

    let checked = tx
        .into_checked_basic(0u32.into(), &consensus_parameters)
        .unwrap();
    let metadata = Metadata::new_test(0, Some(max_gas), Some(tx_id));

    (checked, metadata)
}

#[test]
fn stability_test() {
    let config = Config {
        utxo_validation: false,
        ..Default::default()
    };

    let limit = Limits {
        max_inputs: 4,
        min_outputs: 1,
        max_outputs: 4,
        utxo_id_range: 12,
        gas_limit_range: 1000,
    };

    for _ in 0..100 {
        let seed = thread_rng().next_u64();
        let mut rng = StdRng::seed_from_u64(seed);
        tracing::info!("Running stability test with seed: {}", seed);

        const ROUNDS: usize = 1000;
        let mut errors = 0;

        let mut txpool = TextContext::default().config(config.clone()).build();
        for tip in 0..ROUNDS {
            let (checked, metadata) = some_transaction(limit, tip as u64, &mut rng);
            let result = txpool.insert_single_with_metadata(checked, metadata);
            errors += result.is_err() as usize;
        }

        assert_ne!(ROUNDS, errors);
        tracing::info!("Error rate: {:.02}", errors as f64 / ROUNDS as f64);
    }
}
