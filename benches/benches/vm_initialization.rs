use criterion::{
    black_box,
    Criterion,
    Throughput,
};
use fuel_core_storage::InterpreterStorage;
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
    },
    fuel_types::canonical::Serialize,
    fuel_vm::{
        checked_transaction::{
            Checked,
            IntoChecked,
            Ready,
        },
        constraints::reg_key::Reg,
        consts::VM_MAX_RAM,
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
    let consensus_params = ConsensusParameters::default();
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
        group.throughput(Throughput::Bytes(tx_size as u64));
        group.bench_function(name, |b| {
            b.iter(|| {
                unoptimized_vm_initialization_with_allocating_full_range_of_memory(&tx);
            })
        });
    }

    group.finish();
}

fn unoptimized_vm_initialization_with_allocating_full_range_of_memory(
    ready_tx: &Ready<Script>,
) {
    let vm = black_box(Interpreter::<_, Script, NotSupportedEcal>::with_memory_storage());

    black_box(initialize_vm_with_allocated_full_range_of_memory(
        black_box(ready_tx.clone()),
        vm,
    ));
}

fn initialize_vm_with_allocated_full_range_of_memory<S>(
    ready_tx: Ready<Script>,
    mut vm: Interpreter<S, Script>,
) -> Interpreter<S, Script>
where
    S: InterpreterStorage,
{
    vm.init_script(ready_tx)
        .expect("Should be able to execute transaction");

    const POWER_OF_TWO_OF_HALF_VM: u64 = 25;
    const VM_MEM_HALF: u64 = 1 << POWER_OF_TWO_OF_HALF_VM;
    assert_eq!(VM_MEM_HALF, VM_MAX_RAM / 2);

    for i in 0..=POWER_OF_TWO_OF_HALF_VM {
        let stack = 1 << i;
        let heap = VM_MAX_RAM - stack;

        vm.memory_mut()
            .grow_stack(stack)
            .expect("Should be able to grow stack");
        vm.memory_mut()
            .grow_heap(Reg::new(&0), heap)
            .expect("Should be able to grow heap");
    }

    vm.memory_mut()
        .grow_heap(Reg::new(&0), 0)
        .expect("Should be able to grow heap");

    vm
}
