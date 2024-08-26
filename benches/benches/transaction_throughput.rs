//! Tests throughput of various transaction types

use criterion::{
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
    SamplingMode,
};
use ed25519_dalek::Signer;
use fuel_core::service::config::Trigger;
use fuel_core_benches::*;
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    fuel_asm::{
        op,
        GMArgs,
        GTFArgs,
        RegId,
    },
    fuel_crypto::*,
    fuel_tx::{
        Finalizable,
        Input,
        Output,
        Script,
        Transaction,
        TransactionBuilder,
    },
    fuel_types::{
        AssetId,
        Immediate12,
        Immediate18,
    },
    fuel_vm::{
        checked_transaction::{
            CheckPredicateParams,
            EstimatePredicates,
        },
        interpreter::MemoryInstance,
    },
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use std::{
    sync::Arc,
    time::Duration,
};
use test_helpers::builder::{
    local_chain_config,
    TestContext,
    TestSetupBuilder,
};

// Use Jemalloc during benchmarks
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn bench_txs<F>(group_id: &str, c: &mut Criterion, f: F)
where
    F: Fn(&mut StdRng) -> Script,
{
    let inner_bench = |c: &mut BenchmarkGroup<WallTime>, n: u64| {
        let id = format!("{}", n);
        c.bench_function(id.as_str(), |b| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let _drop = rt.enter();

            let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

            let mut transactions = vec![];
            for _ in 0..n {
                transactions.push(f(&mut rng));
            }

            let mut test_builder = TestSetupBuilder::new(2322);
            // setup genesis block with coins that transactions can spend
            test_builder.config_coin_inputs_from_transactions(
                &transactions.iter().collect::<Vec<_>>(),
            );
            // disable automated block production
            test_builder.trigger = Trigger::Never;
            test_builder.utxo_validation = true;
            test_builder.gas_limit = Some(10_000_000_000);

            // spin up node
            let transactions: Vec<Transaction> =
                transactions.into_iter().map(|tx| tx.into()).collect();
            let transactions = Arc::new(transactions);

            b.to_async(&rt).iter_custom(|iters| {
                let mut elapsed_time = Duration::default();
                let test_builder = test_builder.clone();
                let transactions = transactions.clone();

                async move {
                    for _ in 0..iters {
                        let mut test_builder = test_builder.clone();
                        let sealed_block = {
                            let transactions = transactions
                                .iter()
                                .map(|tx| Arc::new(tx.clone()))
                                .collect();
                            // start the producer node
                            let TestContext { srv, client, .. } =
                                test_builder.finalize().await;

                            // insert all transactions
                            let results =
                                srv.shared.txpool_shared_state.insert(transactions).await;
                            for result in results {
                                let result = result.expect("Should insert transaction");
                                assert_eq!(result.removed.len(), 0);
                            }
                            let _ = client.produce_blocks(1, None).await;

                            // sanity check block to ensure the transactions were actually processed
                            let block = srv
                                .shared
                                .database
                                .on_chain()
                                .latest_view()
                                .unwrap()
                                .get_sealed_block_by_height(&1.into())
                                .unwrap()
                                .unwrap();
                            assert_eq!(
                                block.entity.transactions().len(),
                                (n + 1) as usize
                            );
                            block
                        };

                        // start the validator node
                        let TestContext { srv, .. } = test_builder.finalize().await;

                        let start = std::time::Instant::now();

                        srv.shared
                            .block_importer
                            .execute_and_commit(sealed_block)
                            .await
                            .expect("Should validate the block");

                        elapsed_time += start.elapsed();
                    }
                    elapsed_time
                }
            });
        });
    };

    let mut group = c.benchmark_group(group_id);

    for i in [100, 500, 1000, 1500] {
        group.throughput(criterion::Throughput::Elements(i));
        group.sampling_mode(SamplingMode::Flat);
        group.sample_size(10);
        inner_bench(&mut group, i);
    }

    group.finish();
}

fn signed_transfers(c: &mut Criterion) {
    let generator = |rng: &mut StdRng| {
        TransactionBuilder::script(vec![], vec![])
            .script_gas_limit(10000)
            .add_unsigned_coin_input(
                SecretKey::random(rng),
                rng.gen(),
                1000,
                Default::default(),
                Default::default(),
            )
            .add_unsigned_coin_input(
                SecretKey::random(rng),
                rng.gen(),
                1000,
                Default::default(),
                Default::default(),
            )
            .add_output(Output::coin(rng.gen(), 50, AssetId::default()))
            .add_output(Output::change(rng.gen(), 0, AssetId::default()))
            .finalize()
    };
    bench_txs("signed transfers", c, generator);
}

fn predicate_transfers(c: &mut Criterion) {
    let generator = |rng: &mut StdRng| {
        let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
        let owner = Input::predicate_owner(&predicate);

        let mut tx = TransactionBuilder::script(vec![], vec![])
            .add_input(Input::coin_predicate(
                rng.gen(),
                owner,
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
                predicate.clone(),
                vec![],
            ))
            .add_input(Input::coin_predicate(
                rng.gen(),
                owner,
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
                predicate,
                vec![],
            ))
            .add_output(Output::coin(rng.gen(), 50, AssetId::default()))
            .add_output(Output::change(rng.gen(), 0, AssetId::default()))
            .finalize();
        tx.estimate_predicates(&checked_parameters(), MemoryInstance::new())
            .expect("Predicate check failed");
        tx
    };
    bench_txs("predicate transfers", c, generator);
}

fn predicate_transfers_eck1(c: &mut Criterion) {
    let generator = |rng: &mut StdRng| {
        let secret = SecretKey::random(rng);
        let public = secret.public_key();

        let message = b"The gift of words is the gift of deception and illusion.";
        let message = Message::new(message);

        let signature = Signature::sign(&secret, &message);

        let predicate = vec![
            op::gm_args(0x20, GMArgs::GetVerifyingPredicate),
            op::gtf_args(0x20, 0x20, GTFArgs::InputCoinPredicateData),
            op::addi(0x21, 0x20, signature.as_ref().len() as Immediate12),
            op::addi(0x22, 0x21, message.as_ref().len() as Immediate12),
            op::movi(0x10, PublicKey::LEN as Immediate18),
            op::aloc(0x10),
            op::move_(0x11, RegId::HP),
            op::eck1(0x11, 0x20, 0x21),
            op::meq(0x12, 0x22, 0x11, 0x10),
            op::ret(0x12),
        ]
        .into_iter()
        .collect::<Vec<u8>>();
        let owner = Input::predicate_owner(&predicate);

        let predicate_data: Vec<u8> = signature
            .as_ref()
            .iter()
            .copied()
            .chain(message.as_ref().iter().copied())
            .chain(public.as_ref().iter().copied())
            .collect();

        let mut tx = TransactionBuilder::script(vec![], vec![])
            .script_gas_limit(10000)
            .add_input(Input::coin_predicate(
                rng.gen(),
                owner,
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
                predicate.clone(),
                predicate_data.clone(),
            ))
            .add_input(Input::coin_predicate(
                rng.gen(),
                owner,
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
                predicate,
                predicate_data,
            ))
            .add_output(Output::coin(rng.gen(), 50, AssetId::default()))
            .add_output(Output::change(rng.gen(), 0, AssetId::default()))
            .finalize();
        tx.estimate_predicates(&checked_parameters(), MemoryInstance::new())
            .expect("Predicate check failed");
        tx
    };
    bench_txs("predicate transfers eck1", c, generator);
}

fn predicate_transfers_ed19(c: &mut Criterion) {
    let generator = |rng: &mut StdRng| {
        let ed19_secret = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng {});
        let public = ed19_secret.verifying_key();

        let message = b"The gift of words is the gift of deception and illusion.";
        let message = Message::new(message);

        let signature = ed19_secret.sign(&*message).to_bytes();

        let predicate = vec![
            op::gm_args(0x20, GMArgs::GetVerifyingPredicate),
            op::gtf_args(0x20, 0x20, GTFArgs::InputCoinPredicateData),
            op::addi(0x21, 0x20, PublicKey::LEN as Immediate12),
            op::addi(0x22, 0x21, signature.len() as Immediate12),
            op::movi(0x24, message.as_ref().len() as Immediate18),
            op::ed19(0x20, 0x21, 0x22, 0x24),
            op::eq(0x12, RegId::ERR, RegId::ONE),
            op::ret(0x12),
        ]
        .into_iter()
        .collect::<Vec<u8>>();
        let owner = Input::predicate_owner(&predicate);

        let predicate_data: Vec<u8> = public
            .to_bytes()
            .iter()
            .copied()
            .chain(
                signature
                    .iter()
                    .copied()
                    .chain(message.as_ref().iter().copied()),
            )
            .collect();

        let mut tx = TransactionBuilder::script(vec![], vec![])
            .script_gas_limit(10000)
            .add_input(Input::coin_predicate(
                rng.gen(),
                owner,
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
                predicate.clone(),
                predicate_data.clone(),
            ))
            .add_input(Input::coin_predicate(
                rng.gen(),
                owner,
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
                predicate,
                predicate_data,
            ))
            .add_output(Output::coin(rng.gen(), 50, AssetId::default()))
            .add_output(Output::change(rng.gen(), 0, AssetId::default()))
            .finalize();
        tx.estimate_predicates(&checked_parameters(), MemoryInstance::new())
            .expect("Predicate check failed");
        tx
    };
    bench_txs("predicate transfers ed19", c, generator);
}

criterion_group!(
    benches,
    signed_transfers,
    predicate_transfers,
    predicate_transfers_eck1,
    predicate_transfers_ed19
);
criterion_main!(benches);

fn checked_parameters() -> CheckPredicateParams {
    local_chain_config().consensus_parameters.into()
}
