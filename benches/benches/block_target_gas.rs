use block_target_gas_set::{
    alu::run_alu,
    contract::run_contract,
    crypto::run_crypto,
    flow::run_flow,
    memory::run_memory,
};
use criterion::{
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
};
use ed25519_dalek::Signer;
use fuel_core::service::{
    config::Trigger,
    Config,
    FuelService,
    ServiceTrait,
};
use rand::SeedableRng;

use ethnum::U256;
use fuel_core::txpool::types::Word;
use fuel_core_benches::*;
use fuel_core_chain_config::ContractConfig;
use fuel_core_storage::{
    tables::ContractsRawCode,
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
        Input,
        Output,
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
    },
};

mod utils;

mod block_target_gas_set;

use utils::{
    make_u128,
    make_u256,
};

// Use Jemalloc during benchmarks
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

const STATE_SIZE: u64 = 10_000_000;
const TARGET_BLOCK_GAS_LIMIT: u64 = 262143;
const BASE: u64 = 10_000;

pub struct SanityBenchmarkFactory;

pub struct SharedSanityBenchmarkFactory {
    service: FuelService,
    rt: tokio::runtime::Runtime,
    contract_id: ContractId,
    rng: rand::rngs::StdRng,
}

impl SanityBenchmarkFactory {
    /// Creates a factory for benchmarks that share a service with a contract, `contract_id`, pre-
    /// deployed.
    pub fn new_shared(contract_id: ContractId) -> SharedSanityBenchmarkFactory {
        let (service, rt) = service_with_contract_id(contract_id);
        let rng = rand::rngs::StdRng::seed_from_u64(2322u64);
        SharedSanityBenchmarkFactory {
            service,
            rt,
            contract_id,
            rng,
        }
    }
}

impl SharedSanityBenchmarkFactory {
    pub fn build(&mut self) -> SanityBenchmark {
        SanityBenchmark {
            service: &mut self.service,
            rt: &self.rt,
            rng: &mut self.rng,
            has_contract_input: true,
            extra_inputs: vec![],
            extra_outputs: vec![],
        }
    }

    pub fn build_with_new_contract(
        &mut self,
        contract_instructions: Vec<Instruction>,
    ) -> SanityBenchmark {
        replace_contract_in_service(
            &mut self.service,
            &self.contract_id,
            contract_instructions,
        );
        self.build()
    }
}

pub struct SanityBenchmark<'a> {
    service: &'a mut FuelService,
    rt: &'a tokio::runtime::Runtime,
    rng: &'a mut rand::rngs::StdRng,
    has_contract_input: bool,
    extra_inputs: Vec<Input>,
    extra_outputs: Vec<Output>,
}

impl<'a> SanityBenchmark<'a> {
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
            VmBench::CONTRACT,
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
    group.bench_function(id, |b| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _drop = rt.enter();

        let database = Database::rocksdb();
        let mut config = Config::local_node();
        config.chain_conf.consensus_parameters.tx_params.max_gas_per_tx = TARGET_BLOCK_GAS_LIMIT;
        config
            .chain_conf
            .consensus_parameters
            .predicate_params
            .max_gas_per_predicate = TARGET_BLOCK_GAS_LIMIT;
        config.chain_conf.block_gas_limit = TARGET_BLOCK_GAS_LIMIT;
        config.utxo_validation = false;
        config.block_production = Trigger::Instant;

        let service = fuel_core::service::FuelService::new(database, config.clone())
            .expect("Unable to start a FuelService");
        service.start().expect("Unable to start the service");
        let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

        b.to_async(&rt).iter(|| {
            let shared = service.shared.clone();

            let tx = fuel_core_types::fuel_tx::TransactionBuilder::script(
                script.clone().into_iter().collect(),
                script_data.clone(),
            )
                .gas_limit(TARGET_BLOCK_GAS_LIMIT - BASE)
                .gas_price(1)
                .add_unsigned_coin_input(
                    SecretKey::random(&mut rng),
                    rng.gen(),
                    u64::MAX / 2,
                    AssetId::BASE,
                    Default::default(),
                    Default::default(),
                )
                .finalize_as_transaction();
            async move {
                let tx_id = tx.id(&config.chain_conf.consensus_parameters.chain_id);

                let mut sub = shared.block_importer.block_importer.subscribe();
                shared
                    .txpool
                    .insert(vec![std::sync::Arc::new(tx)])
                    .await
                    .into_iter()
                    .next()
                    .expect("Should be at least 1 element")
                    .expect("Should include transaction successfully");
                let res = sub.recv().await.expect("Should produce a block");
                assert_eq!(res.tx_status.len(), 2);
                assert_eq!(res.sealed_block.entity.transactions().len(), 2);
                assert_eq!(res.tx_status[0].id, tx_id);

                let fuel_core_types::services::executor::TransactionExecutionResult::Failed {
                    reason,
                    ..
                } = &res.tx_status[0].result
                    else {
                        panic!("The execution should fails with out of gas")
                    };
                assert!(reason.contains("OutOfGas"));
            }
        })
    });
}

/// Sets up a service with a full database. Returns the service with the associated Runtime.
/// The size of the database can be overridden with the `STATE_SIZE` environment variable.
fn service_with_contract_id(
    contract_id: ContractId,
) -> (fuel_core::service::FuelService, tokio::runtime::Runtime) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _drop = rt.enter();
    let mut database = Database::rocksdb();
    let mut config = Config::local_node();
    config
        .chain_conf
        .consensus_parameters
        .tx_params
        .max_gas_per_tx = TARGET_BLOCK_GAS_LIMIT;
    config.chain_conf.initial_state.as_mut().unwrap().contracts =
        Some(vec![ContractConfig {
            contract_id,
            code: vec![],
            salt: Default::default(),
            state: None,
            balances: None,
            tx_id: None,
            output_index: None,
            tx_pointer_block_height: None,
            tx_pointer_tx_idx: None,
        }]);

    config
        .chain_conf
        .consensus_parameters
        .predicate_params
        .max_gas_per_predicate = TARGET_BLOCK_GAS_LIMIT;
    // Some txs (e.g. smo) are too big so increasing this arbitrarily
    config.chain_conf.block_gas_limit = TARGET_BLOCK_GAS_LIMIT * 3;
    config.utxo_validation = false;
    config.block_production = Trigger::Instant;

    // Override state size if the env var is set
    let state_size = std::env::var_os("STATE_SIZE")
        .map(|value| {
            let value = value.to_str().unwrap();
            let value = value.parse::<u64>().unwrap();
            println!("Overriding state size with {}", value);
            value
        })
        .unwrap_or(STATE_SIZE);

    database
        .init_contract_state(
            &contract_id,
            (0..state_size).map(|k| {
                let mut key = Bytes32::zeroed();
                key.as_mut()[..8].copy_from_slice(&k.to_be_bytes());
                (key, key)
            }),
        )
        .unwrap();
    database
        .init_contract_balances(
            &contract_id,
            (0..state_size).map(|k| {
                let key = k / 2;
                let mut sub_id = Bytes32::zeroed();
                sub_id.as_mut()[..8].copy_from_slice(&key.to_be_bytes());

                let asset = if k % 2 == 0 {
                    VmBench::CONTRACT.asset_id(&sub_id)
                } else {
                    AssetId::new(*sub_id)
                };
                (asset, key + 1_000)
            }),
        )
        .unwrap();

    let service = fuel_core::service::FuelService::new(database, config.clone())
        .expect("Unable to start a FuelService");
    service.start().expect("Unable to start the service");
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
    service: &fuel_core::service::FuelService,
    contract_id: ContractId,
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
                .gas_limit(TARGET_BLOCK_GAS_LIMIT - BASE)
                .gas_price(1)
                .add_unsigned_coin_input(
                    SecretKey::random(rng),
                    rng.gen(),
                    u32::MAX as u64,
                    AssetId::BASE,
                    Default::default(),
                    Default::default(),
                );
                let input_count = tx_builder.inputs().len();

                let contract_input = Input::contract(
                    UtxoId::default(),
                    Bytes32::zeroed(),
                    Bytes32::zeroed(),
                    TxPointer::default(),
                    contract_id,
                );
                let contract_output = Output::contract(input_count as u8, Bytes32::zeroed(), Bytes32::zeroed());

                tx_builder
                    .add_input(contract_input)
                    .add_output(contract_output);

            for input in &extra_inputs {
                tx_builder.add_input(input.clone());
            }

            for output in &extra_outputs {
                tx_builder.add_output(*output);
            }
            let mut tx = tx_builder.finalize_as_transaction();
            tx.estimate_predicates(&shared.config.chain_conf.consensus_parameters.clone().into()).unwrap();
            async move {
                let tx_id = tx.id(&shared.config.chain_conf.consensus_parameters.chain_id);

                let mut sub = shared.block_importer.block_importer.subscribe();
                shared
                    .txpool
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

                let fuel_core_types::services::executor::TransactionExecutionResult::Failed {
                    reason,
                    ..
                } = &res.tx_status[0].result
                    else {
                        panic!("The execution should fails with out of gas")
                    };
                assert!(reason.contains("OutOfGas"));
            }
        })
    });
}

fn block_target_gas(c: &mut Criterion) {
    let mut group = c.benchmark_group("block target estimation");

    run(
        "Script with meq opcode and infinite loop",
        &mut group,
        [
            op::movi(0x10, (1 << 18) - 1),
            op::meq(0x11, RegId::SP, RegId::SP, 0x10),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "Script with logd opcode and infinite loop",
        &mut group,
        [
            op::movi(0x10, (1 << 18) - 1),
            op::logd(RegId::ZERO, RegId::ZERO, RegId::ZERO, 0x10),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "Script with gtf opcode and infinite loop",
        &mut group,
        [
            op::gtf(0x10, RegId::ZERO, GTFArgs::InputCoinOwner as u16),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run_alu(&mut group);

    run_contract(&mut group);

    run_crypto(&mut group);

    run_flow(&mut group);

    run_memory(&mut group);

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
        op::movi(0x12, TARGET_BLOCK_GAS_LIMIT as u32),
    ]
}

fn call_contract_repeat() -> Vec<Instruction> {
    let mut instructions = setup_instructions();
    instructions.extend(vec![
        op::call(0x10, RegId::ZERO, 0x11, 0x12),
        op::jmpb(RegId::ZERO, 0),
    ]);
    instructions
}

fn call_contract_once() -> Vec<Instruction> {
    let mut instructions = setup_instructions();
    instructions.extend(vec![op::call(0x10, RegId::ZERO, 0x11, 0x12)]);
    instructions
}

criterion_group!(benches, block_target_gas);
criterion_main!(benches);
