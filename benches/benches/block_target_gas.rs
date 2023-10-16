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
    ServiceTrait,
};
use rand::SeedableRng;

use ethnum::U256;
use fuel_core_benches::*;
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
    fuel_tx::UniqueIdentifier,
    fuel_types::AssetId,
};

#[path = "utils.rs"]
mod utils;

use utils::{
    make_u128,
    make_u256,
};

// Use Jemalloc during benchmarks
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

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
        const TARGET_BLOCK_GAS_LIMIT: u64 = 100_000;
        const BASE: u64 = 10_000;

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
                // Infinite loop
                script.clone().into_iter().collect(),
                script_data.clone(),
            )
            .gas_limit(TARGET_BLOCK_GAS_LIMIT - BASE)
            .gas_price(1)
            .add_unsigned_coin_input(
                SecretKey::random(&mut rng),
                rng.gen(),
                u64::MAX,
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
                assert_eq!(res.tx_status[1].id, tx_id);

                let fuel_core_types::services::executor::TransactionExecutionResult::Failed {
                    reason,
                    ..
                } = &res.tx_status[1].result
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

    let message = fuel_core_types::fuel_crypto::Message::new(b"foo");
    let ecr1_secret = p256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let ecr1_signature = secp256r1::sign_prehashed(&ecr1_secret, &message)
        .expect("Failed to sign with secp256r1");

    run(
        "Script with ecr1 opcode and infinite loop",
        &mut group,
        [
            op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
            op::addi(
                0x21,
                0x20,
                ecr1_signature.as_ref().len().try_into().unwrap(),
            ),
            op::addi(0x22, 0x21, message.as_ref().len().try_into().unwrap()),
            op::movi(0x10, PublicKey::LEN.try_into().unwrap()),
            op::aloc(0x10),
            op::move_(0x11, RegId::HP),
            op::ecr1(0x11, 0x20, 0x21),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        ecr1_signature
            .as_ref()
            .iter()
            .chain(message.as_ref())
            .copied()
            .collect(),
    );

    let ed19_keypair =
        ed25519_dalek::Keypair::generate(&mut ed25519_dalek_old_rand::rngs::OsRng {});
    let ed19_signature = ed19_keypair.sign(&*message);

    run(
        "Script with ed19 opcode and infinite loop",
        &mut group,
        [
            op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
            op::addi(
                0x21,
                0x20,
                ed19_keypair.public.as_ref().len().try_into().unwrap(),
            ),
            op::addi(
                0x22,
                0x21,
                ed19_signature.as_ref().len().try_into().unwrap(),
            ),
            op::addi(0x22, 0x21, message.as_ref().len().try_into().unwrap()),
            op::movi(0x10, ed25519_dalek::PUBLIC_KEY_LENGTH.try_into().unwrap()),
            op::aloc(0x10),
            op::move_(0x11, RegId::HP),
            op::ed19(0x20, 0x21, 0x22),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        ed19_keypair
            .public
            .as_ref()
            .iter()
            .chain(ed19_signature.as_ref())
            .chain(message.as_ref())
            .copied()
            .collect(),
    );

    // The test is supper long because we don't use `DependentCost` for k256 opcode
    // run(
    //     "Script with k256 opcode and infinite loop",
    //     &mut group,
    //     [
    //         op::movi(0x10, 1 << 18 - 1),
    //         op::aloc(0x10),
    //         op::k256(RegId::HP, RegId::ZERO, 0x10),
    //         op::jmpb(RegId::ZERO, 0),
    //     ]
    //     .to_vec(),
    // );

    run_alu(&mut group);

    group.finish();
}

fn run_alu(group: &mut BenchmarkGroup<WallTime>) {
    run(
        "add opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::add(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "addi opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::addi(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "aloc opcode",
        group,
        [op::aloc(0x10), op::jmpb(RegId::ZERO, 0)].to_vec(),
        vec![],
    );

    run(
        "and opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::and(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "andi opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::andi(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "div opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::div(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "divi opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::divi(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "eq opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::eq(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "exp opcode",
        group,
        [
            op::movi(0x11, 23),
            op::movi(0x12, 11),
            op::exp(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "expi opcode",
        group,
        [
            op::movi(0x11, 23),
            op::expi(0x10, 0x11, 11),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "gt opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::gt(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    {
        let data = vec![0u8; 32];
        run(
            "gtf opcode",
            group,
            [
                op::gtf_args(0x10, RegId::ZERO, GTFArgs::ScriptData),
                op::jmpb(RegId::ZERO, 0),
            ]
            .to_vec(),
            data,
        );
    }

    run(
        "lt opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::lt(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "mlog opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::mlog(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "mod opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::mod_(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "modi opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::modi(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "move opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::move_(0x10, 0x11),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "movi opcode",
        group,
        [op::movi(0x10, 27), op::jmpb(RegId::ZERO, 0)].to_vec(),
        vec![],
    );

    run(
        "mroo opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::mroo(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "mul opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::mul(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "muli opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::muli(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "mldv opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 3),
            op::movi(0x13, 2),
            op::mldv(0x10, 0x11, 0x12, 0x13),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "noop opcode",
        group,
        [op::noop(), op::jmpb(RegId::ZERO, 0)].to_vec(),
        vec![],
    );

    run(
        "not opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::not(0x10, 0x11),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "or opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::or(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "ori opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::ori(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "sll opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 3),
            op::sll(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "slli opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::slli(0x10, 0x11, 3),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "srl opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 3),
            op::srl(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "srli opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::srli(0x10, 0x11, 3),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "sub opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::sub(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "subi opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::subi(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "xor opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::movi(0x12, 27),
            op::xor(0x10, 0x11, 0x12),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    run(
        "xori opcode",
        group,
        [
            op::movi(0x11, 100000),
            op::xori(0x10, 0x11, 27),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        vec![],
    );

    let mut wideint_prepare = Vec::new();
    wideint_prepare.extend(make_u128(0x10, 0));
    wideint_prepare.extend(make_u128(0x11, u128::MAX));
    wideint_prepare.extend(make_u128(0x12, u128::MAX / 2 + 1));
    wideint_prepare.extend(make_u128(0x13, u128::MAX - 158)); // prime
    wideint_prepare.extend(make_u128(0x14, u64::MAX.into()));

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wdcm_args(
            0x10,
            0x12,
            0x13,
            CompareArgs {
                mode: CompareMode::LTE,
                indirect_rhs: true,
            },
        ),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("wdcm opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wdop_args(
            0x10,
            0x13,
            0x12,
            MathArgs {
                op: MathOp::SUB,
                indirect_rhs: true,
            },
        ),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("wdop opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wdml_args(
            0x10,
            0x14,
            0x14,
            MulArgs {
                indirect_lhs: true,
                indirect_rhs: true,
            },
        ),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("wdml opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wddv_args(0x10, 0x12, 0x13, DivArgs { indirect_rhs: true }),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("wddv opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([op::wdmd(0x10, 0x12, 0x13, 0x13), op::jmpb(RegId::ZERO, 0)]);
    run("wdmd opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([op::wdam(0x10, 0x12, 0x13, 0x13), op::jmpb(RegId::ZERO, 0)]);
    run("wdam opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([op::wdmm(0x10, 0x12, 0x13, 0x13), op::jmpb(RegId::ZERO, 0)]);
    run("wdmm opcode", group, instructions, vec![]);

    // Wideint operations: 256 bit
    let mut wideint_prepare = Vec::new();
    wideint_prepare.extend(make_u256(0x10, U256::ZERO));
    wideint_prepare.extend(make_u256(0x11, U256::MAX));
    wideint_prepare.extend(make_u256(0x12, U256::MAX / 2 + 1));
    wideint_prepare.extend(make_u256(0x13, U256::MAX - 188)); // prime
    wideint_prepare.extend(make_u256(0x14, u128::MAX.into()));

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wqcm_args(
            0x10,
            0x12,
            0x13,
            CompareArgs {
                mode: CompareMode::LTE,
                indirect_rhs: true,
            },
        ),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("wqcm opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wqop_args(
            0x10,
            0x13,
            0x12,
            MathArgs {
                op: MathOp::SUB,
                indirect_rhs: true,
            },
        ),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("wqop opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wqml_args(
            0x10,
            0x14,
            0x14,
            MulArgs {
                indirect_lhs: true,
                indirect_rhs: true,
            },
        ),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("wqml opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([
        op::wqdv_args(0x10, 0x12, 0x13, DivArgs { indirect_rhs: true }),
        op::jmpb(RegId::ZERO, 0),
    ]);
    run("wqdv opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([op::wqmd(0x10, 0x12, 0x13, 0x13), op::jmpb(RegId::ZERO, 0)]);
    run("wqmd opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([op::wqam(0x10, 0x12, 0x13, 0x13), op::jmpb(RegId::ZERO, 0)]);
    run("wqam opcode", group, instructions, vec![]);

    let mut instructions = wideint_prepare.clone();
    instructions.extend([op::wqmm(0x10, 0x12, 0x13, 0x13), op::jmpb(RegId::ZERO, 0)]);
    run("wqmm opcode", group, instructions, vec![]);
}

criterion_group!(benches, block_target_gas);
criterion_main!(benches);
