use block_target_gas_set::{
    alu::run_alu,
    contract::run_contract,
    crypto::run_crypto,
    flow::run_flow,
    memory::run_memory,
    other::run_other,
};
use criterion::{
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
};
use ed25519_dalek::Signer;
use ethnum::U256;
use fuel_core::{
    combined_database::CombinedDatabase,
    database::{
        balances::BalancesInitializer,
        state::StateInitializer,
        Database,
    },
    service::{
        config::Trigger,
        Config,
        FuelService,
    },
    txpool::types::Word,
};
use fuel_core_benches::{
    default_gas_costs::default_gas_costs,
    *,
};
use fuel_core_chain_config::{
    ChainConfig,
    ContractConfig,
    StateConfig,
};
use fuel_core_storage::{
    tables::ContractsRawCode,
    vm_storage::IncreaseStorageKey,
    StorageAsMut,
};
use fuel_core_types::{
    fuel_asm::{
        op,
        wideint::{
            CompareArgs,
            CompareMode,
            DivArgs,
            MathArgs,
            MathOp,
            MulArgs,
        },
        GTFArgs,
        Instruction,
        RegId,
    },
    fuel_crypto::{
        secp256r1,
        *,
    },
    fuel_tx::{
        ContractIdExt,
        FeeParameters,
        GasCosts,
        Input,
        Output,
        PredicateParameters,
        TxParameters,
        TxPointer,
        UniqueIdentifier,
        UtxoId,
    },
    fuel_types::{
        AssetId,
        Bytes32,
        ContractId,
    },
    fuel_vm::{
        checked_transaction::EstimatePredicates,
        consts::WORD_SIZE,
        interpreter::MemoryInstance,
    },
    services::executor::TransactionExecutionResult,
};
use rand::SeedableRng;
use utils::{
    make_u128,
    make_u256,
};

mod utils;

mod block_target_gas_set;

// Use Jemalloc during benchmarks
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

const TARGET_BLOCK_GAS_LIMIT: u64 = 1_000_000;
const BASE: u64 = 100_000;

pub struct SanityBenchmarkRunnerBuilder;

pub struct SharedSanityBenchmarkRunnerBuilder {
    service: FuelService,
    rt: tokio::runtime::Runtime,
    contract_id: ContractId,
    rng: rand::rngs::StdRng,
}

pub struct MultiContractBenchmarkRunnerBuilder {
    service: FuelService,
    rt: tokio::runtime::Runtime,
    contract_ids: Vec<ContractId>,
    rng: rand::rngs::StdRng,
}

impl SanityBenchmarkRunnerBuilder {
    /// Creates a factory for benchmarks that share a service with a contract, `contract_id`, pre-
    /// deployed.
    /// The size of the database can be overridden with the `STATE_SIZE` environment variable.
    pub fn new_shared(contract_id: ContractId) -> SharedSanityBenchmarkRunnerBuilder {
        let state_size = crate::utils::get_state_size();
        let (service, rt) = service_with_many_contracts(state_size, vec![contract_id]);
        let rng = rand::rngs::StdRng::seed_from_u64(2322u64);
        SharedSanityBenchmarkRunnerBuilder {
            service,
            rt,
            contract_id,
            rng,
        }
    }

    pub fn new_with_many_contracts(
        contract_ids: Vec<ContractId>,
    ) -> MultiContractBenchmarkRunnerBuilder {
        let state_size = 1000; // Arbitrary small state size
        let (service, rt) = service_with_many_contracts(state_size, contract_ids.clone());
        let rng = rand::rngs::StdRng::seed_from_u64(2322u64);
        MultiContractBenchmarkRunnerBuilder {
            service,
            rt,
            contract_ids,
            rng,
        }
    }
}

impl SharedSanityBenchmarkRunnerBuilder {
    fn build(&mut self) -> SanityBenchmarkRunner {
        SanityBenchmarkRunner {
            service: &mut self.service,
            rt: &self.rt,
            rng: &mut self.rng,
            contract_ids: vec![self.contract_id],
            extra_inputs: vec![],
            extra_outputs: vec![],
        }
    }

    pub fn build_with_new_contract(
        &mut self,
        contract_instructions: Vec<Instruction>,
    ) -> SanityBenchmarkRunner {
        replace_contract_in_service(
            &mut self.service,
            &self.contract_id,
            contract_instructions,
        );
        self.build()
    }
}

impl MultiContractBenchmarkRunnerBuilder {
    pub fn build(&mut self) -> SanityBenchmarkRunner {
        SanityBenchmarkRunner {
            service: &mut self.service,
            rt: &self.rt,
            rng: &mut self.rng,
            contract_ids: self.contract_ids.clone(),
            extra_inputs: vec![],
            extra_outputs: vec![],
        }
    }
}

pub struct SanityBenchmarkRunner<'a> {
    service: &'a mut FuelService,
    rt: &'a tokio::runtime::Runtime,
    rng: &'a mut rand::rngs::StdRng,
    contract_ids: Vec<ContractId>,
    extra_inputs: Vec<Input>,
    extra_outputs: Vec<Output>,
}

impl<'a> SanityBenchmarkRunner<'a> {
    pub fn with_extra_inputs(mut self, extra_inputs: Vec<Input>) -> Self {
        self.extra_inputs = extra_inputs;
        self
    }

    pub fn with_extra_outputs(mut self, extra_outputs: Vec<Output>) -> Self {
        self.extra_outputs = extra_outputs;
        self
    }

    pub fn run(
        self,
        id: &str,
        group: &mut BenchmarkGroup<WallTime>,
        script: Vec<Instruction>,
        script_data: Vec<u8>,
    ) {
        run_with_service_with_extra_inputs(
            id,
            group,
            script,
            script_data,
            self.service,
            self.contract_ids,
            self.rt,
            self.rng,
            self.extra_inputs,
            self.extra_outputs,
        );
    }
}

fn run(
    id: &str,
    group: &mut BenchmarkGroup<WallTime>,
    script: Vec<Instruction>,
    script_data: Vec<u8>,
) {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);
    let contract_ids = vec![];
    let (service, rt) = service_with_many_contracts(0, contract_ids.clone()); // Doesn't need any contracts
    run_with_service_with_extra_inputs(
        id,
        group,
        script,
        script_data,
        &service,
        contract_ids,
        &rt,
        &mut rng,
        vec![],
        vec![],
    );
}

/// Sets up a service with a contract for each contract id. The contract state will be set to
/// `state_size` for each contract.
fn service_with_many_contracts(
    state_size: u64,
    contract_ids: Vec<ContractId>,
) -> (FuelService, tokio::runtime::Runtime) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _drop = rt.enter();
    let mut database = Database::rocksdb_temp();

    let mut chain_config = ChainConfig::local_testnet();

    chain_config.consensus_parameters.set_tx_params(
        TxParameters::default().with_max_gas_per_tx(TARGET_BLOCK_GAS_LIMIT),
    );
    chain_config.consensus_parameters.set_predicate_params(
        PredicateParameters::default().with_max_gas_per_predicate(TARGET_BLOCK_GAS_LIMIT),
    );
    chain_config
        .consensus_parameters
        .set_fee_params(FeeParameters::default().with_gas_per_byte(0));
    chain_config
        .consensus_parameters
        .set_block_gas_limit(TARGET_BLOCK_GAS_LIMIT);
    chain_config
        .consensus_parameters
        .set_gas_costs(GasCosts::new(default_gas_costs()));

    let contract_configs = contract_ids
        .iter()
        .map(|contract_id| ContractConfig {
            contract_id: *contract_id,
            ..Default::default()
        })
        .collect::<Vec<_>>();

    let state_config = StateConfig {
        contracts: contract_configs,
        ..Default::default()
    };
    let mut config = Config::local_node_with_configs(chain_config, state_config);
    config.utxo_validation = false;
    config.block_production = Trigger::Instant;

    let mut storage_key = primitive_types::U256::zero();
    let mut key_bytes = Bytes32::zeroed();

    for contract_id in contract_ids.iter() {
        database
            .init_contract_state(
                contract_id,
                (0..state_size).map(|_| {
                    storage_key.to_big_endian(key_bytes.as_mut());
                    storage_key.increase().unwrap();
                    (key_bytes, key_bytes.to_vec())
                }),
            )
            .unwrap();

        let mut storage_key = primitive_types::U256::zero();
        let mut sub_id = Bytes32::zeroed();
        database
            .init_contract_balances(
                contract_id,
                (0..state_size).map(|k| {
                    storage_key.to_big_endian(sub_id.as_mut());

                    let asset = if k % 2 == 0 {
                        VmBench::CONTRACT.asset_id(&sub_id)
                    } else {
                        let asset_id = AssetId::new(*sub_id);
                        storage_key.increase().unwrap();
                        asset_id
                    };
                    (asset, k / 2 + 1_000)
                }),
            )
            .unwrap();
    }

    let service = rt.block_on(async move {
        FuelService::from_combined_database(
            CombinedDatabase::new(
                database,
                Default::default(),
                Default::default(),
                Default::default(),
            ),
            config.clone(),
        )
        .await
        .expect("Unable to start FuelService")
    });
    (service, rt)
}

// Runs benchmark for `script` with prepared `service` and specified contract (by `contract_id`) which should be
// included in service.
// Also include additional inputs and outputs in transaction
#[allow(clippy::too_many_arguments)]
fn run_with_service_with_extra_inputs(
    id: &str,
    group: &mut BenchmarkGroup<WallTime>,
    script: Vec<Instruction>,
    script_data: Vec<u8>,
    service: &FuelService,
    contract_ids: Vec<ContractId>,
    rt: &tokio::runtime::Runtime,
    rng: &mut rand::rngs::StdRng,
    extra_inputs: Vec<Input>,
    extra_outputs: Vec<Output>,
) {
    group.bench_function(id, |b| {
        b.to_async(rt).iter(|| {
            let shared = service.shared.clone();

            let mut tx_builder = fuel_core_types::fuel_tx::TransactionBuilder::script(
                script.clone().into_iter().collect(),
                script_data.clone(),
            );
            tx_builder
                .script_gas_limit(TARGET_BLOCK_GAS_LIMIT - BASE)
                .add_unsigned_coin_input(
                    SecretKey::random(rng),
                    rng.gen(),
                    u32::MAX as u64,
                    AssetId::BASE,
                    Default::default(),
                );
            for contract_id in &contract_ids {
                let input_count = tx_builder.inputs().len();

                let contract_input = Input::contract(
                    UtxoId::default(),
                    Bytes32::zeroed(),
                    Bytes32::zeroed(),
                    TxPointer::default(),
                    *contract_id,
                );
                let contract_output = Output::contract(
                    input_count as u16,
                    Bytes32::zeroed(),
                    Bytes32::zeroed(),
                );

                tx_builder
                    .add_input(contract_input)
                    .add_output(contract_output);
            }

            for input in &extra_inputs {
                tx_builder.add_input(input.clone());
            }

            for output in &extra_outputs {
                tx_builder.add_output(*output);
            }
            let mut tx = tx_builder.finalize_as_transaction();
            let chain_config = shared.config.snapshot_reader.chain_config().clone();
            tx.estimate_predicates(
                &chain_config.consensus_parameters.clone().into(),
                MemoryInstance::new(),
            )
            .unwrap();
            async move {
                let tx_id = tx.id(&chain_config.consensus_parameters.chain_id());

                let mut sub = shared.block_importer.block_importer.subscribe();
                shared
                    .txpool_shared_state
                    .insert(vec![std::sync::Arc::new(tx)])
                    .await
                    .into_iter()
                    .next()
                    .expect("Should be at least 1 element")
                    .expect("Should include transaction successfully");
                let res = sub.recv().await.expect("Should produce a block");
                assert_eq!(res.tx_status.len(), 2, "res.tx_status: {:?}", res.tx_status);
                assert_eq!(res.sealed_block.entity.transactions().len(), 2);
                assert_eq!(res.tx_status[0].id, tx_id);

                let TransactionExecutionResult::Failed {
                    result, receipts, ..
                } = &res.tx_status[0].result
                else {
                    panic!("The execution should fails with out of gas")
                };
                let reason = TransactionExecutionResult::reason(receipts, result);
                if !reason.contains("OutOfGas") {
                    panic!("The test failed because of {}", reason);
                }
            }
        })
    });
}

fn block_target_gas(c: &mut Criterion) {
    let mut group = c.benchmark_group("block target estimation");

    run_alu(&mut group);

    run_contract(&mut group);

    run_crypto(&mut group);

    run_flow(&mut group);

    run_memory(&mut group);

    run_other(&mut group);

    group.finish();
}

fn replace_contract_in_service(
    service: &mut FuelService,
    contract_id: &ContractId,
    contract_instructions: Vec<Instruction>,
) {
    let contract_bytecode: Vec<_> = contract_instructions
        .iter()
        .flat_map(|x| x.to_bytes())
        .collect();
    service
        .shared
        .database
        .on_chain_mut()
        .storage_as_mut::<ContractsRawCode>()
        .insert(contract_id, &contract_bytecode)
        .unwrap();
}

fn script_data(contract_id: &ContractId, asset_id: &AssetId) -> Vec<u8> {
    contract_id
        .iter()
        .copied()
        .chain((0 as Word).to_be_bytes().iter().copied())
        .chain((0 as Word).to_be_bytes().iter().copied())
        .chain(asset_id.iter().copied())
        .collect()
}

fn setup_instructions() -> Vec<Instruction> {
    vec![
        op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
        op::addi(0x11, 0x10, ContractId::LEN.try_into().unwrap()),
        op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
        op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
        op::movi(0x12, (1 << 18) - 1),
    ]
}

/// Returns a bytecode that contains an infinite loop that increases the `u256` iterator by
/// `1` each iteration. A function expects a closure that returns an opcode that must
/// be called infinitely. The closure should accept one argument -
/// the register where the iterator is stored.
fn u256_iterator_loop(opcode: impl Fn(RegId) -> Instruction) -> Vec<Instruction> {
    u256_iterator_loop_with_step(opcode, 1)
}

/// Returns a bytecode that contains an infinite loop that increases the `u256` iterator by
/// `step` each iteration. A function expects a closure that returns an opcode that must
/// be called infinitely. The closure should accept one argument -
/// the register where the iterator is stored.
fn u256_iterator_loop_with_step(
    opcode: impl Fn(RegId) -> Instruction,
    step: u32,
) -> Vec<Instruction> {
    // Register where we store an iterator.
    let iterator_register = RegId::new(0x20);
    let step_register = RegId::new(0x21);
    vec![
        // Store size of the iterator.
        op::movi(iterator_register, 32),
        // Store step value.
        op::movi(step_register, step),
        // Allocate 32 bytes for u256 iterator.
        op::aloc(iterator_register),
        // Store the address of the u256 iterator into `iterator_register`.
        op::move_(iterator_register, RegId::HP),
        // We need to pad number of isntruciton to be 8-byte aligned.
        op::noop(),
        // Execute benchmarking opcode.
        opcode(iterator_register),
        // Increment the iterator by one.
        op::wqop(
            iterator_register,
            iterator_register,
            step_register,
            MathOp::ADD as u8,
        ),
        // Jump 4 instructions(jmpb, wqop, opcode, noop) back.
        op::jmpb(RegId::ZERO, 1),
    ]
}

fn call_contract_repeat() -> Vec<Instruction> {
    let mut instructions = setup_instructions();
    instructions.extend(vec![
        op::call(0x10, RegId::ZERO, 0x11, RegId::CGAS),
        op::jmpb(RegId::ZERO, 0),
    ]);
    instructions
}

fn call_contract_once() -> Vec<Instruction> {
    let mut instructions = setup_instructions();
    instructions.extend(vec![op::call(0x10, RegId::ZERO, 0x11, RegId::CGAS)]);
    instructions
}

criterion_group!(benches, block_target_gas);
criterion_main!(benches);
