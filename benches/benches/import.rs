use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    Criterion,
};
use fuel_core_benches::import::{
    PressureBlockImporterPort,
    PressureConsensusPort,
    PressurePeerToPeerPort,
};
use fuel_core_services::{
    SharedMutex,
    StateWatcher,
};
use fuel_core_sync::{
    import::{
        Config,
        Import,
    },
    state::State,
};
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::{
    runtime::Runtime,
    sync::{
        watch::Sender,
        Notify,
    },
};

type PressureImport =
    Import<PressurePeerToPeerPort, PressureBlockImporterPort, PressureConsensusPort>;

#[derive(Default)]
struct Durations {
    headers: Duration,
    consensus: Duration,
    transactions: Duration,
    executes: Duration,
}

fn create_import(
    shared_state: SharedMutex<State>,
    input: Durations,
    max_get_header_requests: usize,
    max_get_txns_requests: usize,
) -> (
    PressureImport,
    Sender<fuel_core_services::State>,
    StateWatcher,
) {
    let shared_notify = Arc::new(Notify::new());
    let params = Config {
        max_get_header_requests,
        max_get_txns_requests,
    };
    let p2p = Arc::new(PressurePeerToPeerPort::new([
        input.headers,
        input.transactions,
    ]));
    let executor = Arc::new(PressureBlockImporterPort::new(input.executes));
    let consensus = Arc::new(PressureConsensusPort::new(input.consensus));

    let (tx, shutdown) = tokio::sync::watch::channel(fuel_core_services::State::Started);
    let watcher = shutdown.into();
    let import = Import::new(
        shared_state,
        shared_notify,
        params,
        p2p,
        executor,
        consensus,
    );
    (import, tx, watcher)
}

async fn test(import: &PressureImport, shutdown: &mut StateWatcher) {
    import.import(shutdown).await.unwrap();
}

fn import_one(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("import");
    group.bench_function("import one", |b| {
        b.to_async(&rt).iter_custom(|iters| async move {
            let mut elapsed_time = Duration::default();
            for _ in 0..iters {
                let state = State::new(None, 50);
                let shared_state = SharedMutex::new(state);
                let input = Durations {
                    headers: Duration::from_millis(5),
                    consensus: Duration::from_millis(5),
                    transactions: Duration::from_millis(5),
                    executes: Duration::from_millis(10),
                };
                let (import, _tx, mut shutdown) =
                    create_import(shared_state, input, 10, 10);
                import.notify_one();
                let start = std::time::Instant::now();
                black_box(test(&import, &mut shutdown).await);
                elapsed_time += start.elapsed();
            }
            elapsed_time
        })
    });
}

criterion_group!(benches, import_one);
criterion_main!(benches);
