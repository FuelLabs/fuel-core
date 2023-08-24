use criterion::{
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
};
use fuel_core_benches::import::{
    provision_import_test,
    Durations,
    PressureImport,
    SharedCounts,
};
use fuel_core_services::{
    SharedMutex,
    StateWatcher,
};
use fuel_core_sync::state::State;
use std::time::Duration;
use tokio::runtime::Runtime;

async fn execute_import(import: PressureImport, shutdown: &mut StateWatcher) {
    import.import(shutdown).await.unwrap();
}

fn name(n: u32, durations: Durations, buffer_size: usize) -> String {
    format!(
        "import {n} * {d_h}/{d_c}/{d_t}/{d_e} - {sz}",
        n = n,
        d_h = durations.headers.as_millis(),
        d_c = durations.consensus.as_millis(),
        d_t = durations.transactions.as_millis(),
        d_e = durations.executes.as_millis(),
        sz = buffer_size
    )
}

fn bench_imports(c: &mut Criterion) {
    let bench_import = |group: &mut BenchmarkGroup<WallTime>,
                        n: u32,
                        durations: Durations,
                        batch_size: u32,
                        buffer_size: usize| {
        let name = name(n, durations, buffer_size);
        group.bench_function(name, move |b| {
            let rt = Runtime::new().unwrap();
            b.to_async(&rt).iter_custom(|iters| async move {
                let mut elapsed_time = Duration::default();
                for _ in 0..iters {
                    let shared_count = SharedCounts::new(Default::default());
                    let state = State::new(None, n);
                    let shared_state = SharedMutex::new(state);
                    let (import, _tx, mut shutdown) = provision_import_test(
                        shared_count.clone(),
                        shared_state,
                        durations,
                        batch_size,
                        buffer_size,
                        buffer_size,
                    );
                    import.notify_one();
                    let start = std::time::Instant::now();
                    execute_import(import, &mut shutdown).await;
                    elapsed_time += start.elapsed();
                }
                elapsed_time
            })
        });
    };

    let mut group = c.benchmark_group("import");

    let n = 100;
    let durations = Durations {
        headers: Duration::from_millis(5),
        consensus: Duration::from_millis(5),
        transactions: Duration::from_millis(5),
        executes: Duration::from_millis(10),
    };

    // Header batch size = 10, header/txn buffer size = 10
    bench_import(&mut group, n, durations, 10, 10);

    // Header batch size = 20, header/txn buffer size = 10
    bench_import(&mut group, n, durations, 20, 10);

    // Header batch size = 50, header/txn buffer size = 10
    bench_import(&mut group, n, durations, 20, 10);

    // Header batch size = 10, header/txn buffer size = 20
    bench_import(&mut group, n, durations, 10, 20);

    // Header batch size = 10, header/txn buffer size = 50
    bench_import(&mut group, n, durations, 10, 50);

    // Header batch size = 50, header/txn buffer size = 50
    bench_import(&mut group, n, durations, 10, 20);
}

criterion_group!(benches, bench_imports);
criterion_main!(benches);
