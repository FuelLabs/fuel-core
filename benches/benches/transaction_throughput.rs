//! Tests throughput of various transaction types

use criterion::{
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
};
use fuel_core::service::config::Trigger;
use fuel_core_benches::*;
use fuel_core_types::{
    fuel_crypto::*,
    fuel_tx::{
        Finalizable,
        Output,
        Transaction,
        TransactionBuilder,
    },
    fuel_types::AssetId,
};
use futures::{
    stream::FuturesUnordered,
    FutureExt,
    StreamExt,
};
use rand::SeedableRng;
use std::sync::Arc;
use test_helpers::builder::{
    TestContext,
    TestSetupBuilder,
};

// Use Jemalloc during benchmarks
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn basic_transfers(c: &mut Criterion) {
    let transfer_bench = |c: &mut BenchmarkGroup<WallTime>, n: u32| {
        let id = format!("basic transfers {}", n);
        c.bench_function(id.as_str(), |b| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let _drop = rt.enter();

            let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

            let mut transactions = vec![];
            for _ in 0..n {
                let tx = TransactionBuilder::script(vec![], vec![])
                    .gas_limit(10000)
                    .gas_price(1)
                    .add_unsigned_coin_input(
                        SecretKey::random(&mut rng),
                        rng.gen(),
                        1000,
                        Default::default(),
                        Default::default(),
                        Default::default(),
                    )
                    .add_unsigned_coin_input(
                        SecretKey::random(&mut rng),
                        rng.gen(),
                        1000,
                        Default::default(),
                        Default::default(),
                        Default::default(),
                    )
                    .add_output(Output::coin(rng.gen(), 50, AssetId::default()))
                    .add_output(Output::change(rng.gen(), 0, AssetId::default()))
                    .finalize();
                transactions.push(tx);
            }

            let mut test_builder = TestSetupBuilder::new(2322);
            // setup genesis block with coins that transactions can spend
            test_builder.config_coin_inputs_from_transactions(
                &transactions.iter().collect::<Vec<_>>(),
            );
            // disable automated block production
            test_builder.trigger = Trigger::Never;
            test_builder.utxo_validation = true;

            // spin up node
            let TestContext {
                srv: _dont_drop,
                client,
                ..
            } = rt.block_on(async move { test_builder.finalize().await });
            let transactions: Vec<Transaction> =
                transactions.into_iter().map(|tx| tx.into()).collect();
            let transactions = Arc::new(transactions);
            b.to_async(&rt).iter(|| {
                let client = client.clone();
                let transactions = transactions.clone();
                async move {
                    // insert all transactions
                    let submits = FuturesUnordered::new();
                    for tx in transactions.iter() {
                        submits.push(client.submit(&tx).boxed());
                    }
                    let _: Vec<_> = submits.collect().await;

                    client.produce_blocks(1, None).await
                }
            });
        });
    };

    let mut group = c.benchmark_group("basic transfer throughput");

    transfer_bench(&mut group, 10);

    transfer_bench(&mut group, 100);

    transfer_bench(&mut group, 1000);
}

criterion_group!(benches, basic_transfers);
criterion_main!(benches);
