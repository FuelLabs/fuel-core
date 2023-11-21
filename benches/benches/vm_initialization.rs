use criterion::{
    black_box,
    Criterion,
    Throughput,
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
    fuel_types::canonical::Serialize,
    fuel_vm::{
        checked_transaction::{
            Checked,
            CheckedTransaction,
            IntoChecked,
        },
        interpreter::NotSupportedEcal,
        Interpreter,
    },
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

fn transaction<R: Rng>(
    rng: &mut R,
    script: Vec<u8>,
    script_data: Vec<u8>,
) -> Checked<Script> {
    let mut consensus_params = ConsensusParameters::default();
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
        1_000_000,
        script,
        script_data,
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

    tx
}

pub fn vm_initialization(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(8586);

    let mut group = c.benchmark_group("vm_initialization");

    // Generate N data points
    const N: usize = 16;
    for i in 0..N {
        let increment = 1024 / N;
        let script_size = 1024 * increment * i;
        let script = vec![op::ret(1); script_size / Instruction::SIZE]
            .into_iter()
            .collect();
        let script_data_size = 1024 * increment * i;
        let script_data = vec![255; script_data_size];
        let tx = transaction(&mut rng, script, script_data);
        let size = tx.transaction().size();
        let name = format!("vm_initialization_with_tx_size_{}", size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(name, |b| {
            b.iter(|| {
                let mut vm = black_box(
                    Interpreter::<_, Script, NotSupportedEcal>::with_memory_storage(),
                );
                black_box(vm.init_script(tx.clone()))
                    .expect("Should be able to execute transaction");
            })
        });
    }

    group.finish();
}
