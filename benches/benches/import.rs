use criterion::{
    async_executor::AsyncExecutor,
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
};
use fuel_core_benches::import::{
    PressureBlockImporterPort,
    PressureConsensusPort,
    PressurePeerToPeerPort,
};
use fuel_core_services::SharedMutex;
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
    sync::Notify,
};

#[derive(Default)]
struct Input {
    headers: Duration,
    consensus: Duration,
    transactions: Duration,
    executes: Duration,
}

async fn test() {
    let input = Input {
        headers: Duration::from_millis(5),
        transactions: Duration::from_millis(5),
        executes: Duration::from_millis(10),
        ..Default::default()
    };
    let params = Config {
        max_get_header_requests: 10,
        max_get_txns_requests: 10,
    };
    let state = State::new(None, 50);
    let state = SharedMutex::new(state);

    let p2p = Arc::new(PressurePeerToPeerPort::new([
        input.headers,
        input.transactions,
    ]));
    let executor = Arc::new(PressureBlockImporterPort::new(input.executes));
    let consensus = Arc::new(PressureConsensusPort::new(input.consensus));

    let notify = Arc::new(Notify::new());
    let (_tx, shutdown) = tokio::sync::watch::channel(fuel_core_services::State::Started);
    let mut watcher = shutdown.into();

    let import = Import::new(state, notify, params, p2p, executor, consensus);
    import.notify.notify_one();
    import.import(&mut watcher).await.unwrap();
}

fn import_one(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("import");
    group.bench_function("import one", |b| b.to_async(&rt).iter(|| test()));
}

criterion_group!(benches, import_one);
criterion_main!(benches);
