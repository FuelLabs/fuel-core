use block_target_gas_set::alu::run_alu;
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

mod utils;

mod block_target_gas_set;

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

criterion_group!(benches, block_target_gas);
criterion_main!(benches);
