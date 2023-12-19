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
    consensus_params: &ConsensusParameters,
) -> Checked<Script> {
    let inputs = (0..1)
        .map(|_| {
            Input::coin_predicate(
                rng.gen(),
                rng.gen(),
                rng.gen(),
                AssetId::BASE,
                rng.gen(),
                rng.gen(),
                0,
                vec![255; 1],
                vec![255; 1],
            )
        })
        .collect();

    let outputs = (0..1)
        .map(|_| {
            Output::variable(Default::default(), Default::default(), Default::default())
        })
        .collect();

    Transaction::script(
        1_000_000,
        script,
        script_data,
        Policies::new()
            .with_gas_price(0)
            .with_maturity(0.into())
            .with_max_fee(Word::MAX),
        inputs,
        outputs,
        vec![vec![123; 32].into(); 1],
    )
    .into_checked_basic(Default::default(), consensus_params)
    .expect("Should produce a valid transaction")
}

pub fn vm_initialization(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(8586);

    let mut group = c.benchmark_group("vm_initialization");

    let consensus_params = ConsensusParameters::default();
    let mut i = 5usize;
    loop {
        let size = 8 * (1 << i);
        if size as u64 > consensus_params.script_params.max_script_data_length {
            break
        }
        let script = vec![op::ret(1); size / Instruction::SIZE]
            .into_iter()
            .collect();
        let script_data = vec![255; size];
        let tx = transaction(&mut rng, script, script_data, &consensus_params);
        let tx_size = tx.transaction().size();
        let name = format!("vm_initialization_with_tx_size_{}", tx_size);
        group.throughput(Throughput::Bytes(tx_size as u64));
        group.bench_function(name, |b| {
            b.iter(|| {
                let mut vm = black_box(
                    Interpreter::<_, Script, NotSupportedEcal>::with_memory_storage(),
                );
                black_box(vm.init_script(tx.clone()))
                    .expect("Should be able to execute transaction");
            })
        });
        i += 1;
    }

    group.finish();
}
