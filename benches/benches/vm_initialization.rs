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
        TxParameters,
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
    let mut inputs = (0..1)
        .map(|_| {
            Input::coin_predicate(
                rng.gen(),
                rng.gen(),
                rng.gen(),
                AssetId::BASE,
                rng.gen(),
                0,
                vec![255; 1],
                vec![255; 1],
            )
        })
        .collect::<Vec<_>>();

    const CONTRACTS_NUMBER: u16 = 254;

    for i in 0..CONTRACTS_NUMBER {
        inputs.push(Input::contract(
            rng.gen(),
            rng.gen(),
            rng.gen(),
            rng.gen(),
            [i as u8; 32].into(),
        ));
    }

    let mut outputs = (0..1)
        .map(|_| {
            Output::variable(Default::default(), Default::default(), Default::default())
        })
        .collect::<Vec<_>>();

    for i in 0..CONTRACTS_NUMBER {
        outputs.push(Output::contract(
            i + 1,
            Default::default(),
            Default::default(),
        ));
    }

    Transaction::script(
        1_000_000,
        script,
        script_data,
        Policies::new().with_max_fee(0).with_maturity(0.into()),
        inputs,
        outputs,
        vec![vec![123; 32].into(); 1],
    )
    .into_checked_basic(Default::default(), consensus_params)
    .expect("Should produce a valid transaction")
}

pub fn vm_initialization(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(8586);
    let mut consensus_params = ConsensusParameters::default();
    consensus_params.set_tx_params(TxParameters::default().with_max_size(256 * 1024));
    let mut group = c.benchmark_group("vm_initialization");

    // Increase the size of the script to measure the performance of the VM initialization
    // with a large script. THe largest allowed script is 64 KB = 8 * 2^13 bytes.
    const TX_SIZE_POWER_OF_TWO: usize = 13;

    for i in 5..=TX_SIZE_POWER_OF_TWO {
        let size = 8 * (1 << i);

        let script = vec![op::ret(1); size / Instruction::SIZE]
            .into_iter()
            .collect();
        let script_data = vec![255; size];
        let tx = transaction(&mut rng, script, script_data, &consensus_params);
        let tx_size = tx.transaction().size();
        let tx = tx.test_into_ready();

        let name = format!("vm_initialization_with_tx_size_{}", tx_size);
        let mut vm = black_box(
            Interpreter::<_, _, Script, NotSupportedEcal>::with_memory_storage(),
        );
        group.throughput(Throughput::Bytes(tx_size as u64));
        group.bench_function(name, |b| {
            b.iter(|| {
                #[allow(clippy::unit_arg)]
                black_box(
                    vm.init_script(tx.clone())
                        .expect("Should be able to execute transaction"),
                );
            })
        });
    }

    group.finish();
}
