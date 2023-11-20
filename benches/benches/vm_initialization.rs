use criterion::{
    black_box,
    Criterion,
};
use fuel_core_types::{
    fuel_asm::{
        op,
        Instruction,
    },
    fuel_tx::{
        policies::Policies,
        AssetId,
        ConsensusParameters,
        Input,
        Output,
        Script,
        Transaction,
        Word,
    },
    fuel_vm::{
        checked_transaction::IntoChecked,
        interpreter::NotSupportedEcal,
        Interpreter,
    },
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

pub fn vm_initialization(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(8586);
    let consensus_params = ConsensusParameters::default();

    let inputs = (0..consensus_params.tx_params.max_inputs)
        .map(|_| {
            Input::coin_predicate(
                rng.gen(),
                rng.gen(),
                rng.gen(),
                AssetId::BASE,
                rng.gen(),
                rng.gen(),
                0,
                vec![
                    255;
                    (consensus_params.predicate_params.max_predicate_length
                        / consensus_params.tx_params.max_inputs as u64)
                        as usize
                ],
                vec![
                    255;
                    (consensus_params.predicate_params.max_predicate_data_length
                        / consensus_params.tx_params.max_inputs as u64)
                        as usize
                ],
            )
        })
        .collect();

    let outputs = (0..consensus_params.tx_params.max_outputs)
        .map(|_| {
            Output::variable(Default::default(), Default::default(), Default::default())
        })
        .collect();

    let tx = Transaction::script(
        1000000,
        vec![
            op::ret(1);
            consensus_params.script_params.max_script_length as usize / Instruction::SIZE
        ]
        .into_iter()
        .collect(),
        vec![255; consensus_params.script_params.max_script_data_length as usize],
        Policies::new()
            .with_gas_price(0)
            .with_maturity(0.into())
            .with_max_fee(Word::MAX),
        inputs,
        outputs,
        vec![vec![123; 100].into(); consensus_params.tx_params.max_witnesses as usize],
    )
    .into_checked_basic(Default::default(), &consensus_params)
    .expect("Should produce a valid transaction");

    let mut group = c.benchmark_group("vm_initialization");

    group.bench_function(format!("vm_initialization"), |b| {
        b.iter(|| {
            let mut vm = black_box(
                Interpreter::<_, Script, NotSupportedEcal>::with_memory_storage(),
            );
            black_box(vm.init_script(tx.clone()))
                .expect("Should be able to execute transaction");
        })
    });

    group.finish();
}
