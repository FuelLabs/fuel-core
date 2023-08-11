use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
};
use fuel_core_benches::import::{
    create_import,
    Count,
    Durations,
    PressureBlockImporterPort,
    PressureConsensusPort,
    PressureImport,
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
    fmt::{
        Display,
        Formatter,
    },
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

#[derive(Clone, Copy)]
enum Version {
    V1,
    V2,
    V3,
    V4,
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::V1 => write!(f, "v1"),
            Version::V2 => write!(f, "v2"),
            Version::V3 => write!(f, "v3"),
            Version::V4 => write!(f, "v4"),
        }
    }
}

async fn import_version_switch(
    import: PressureImport,
    version: Version,
    shutdown: &mut StateWatcher,
) {
    match version {
        Version::V1 => import.import(shutdown).await.unwrap(),
        Version::V2 => import.import_v2(shutdown).await.unwrap(),
        Version::V3 => import.import_v3(shutdown).await.unwrap(),
        Version::V4 => import.import_v4(shutdown).await.unwrap(),
    };
}

fn name(version: Version, n: usize, durations: Durations, buffer_size: usize) -> String {
    format!(
        "import {version} - {n} * {d_h}/{d_c}/{d_t}/{d_e} - {sz}",
        version = version,
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
                        version: Version,
                        n: usize,
                        durations: Durations,
                        buffer_size: usize| {
        let name = name(version, n, durations, buffer_size);
        group.bench_function(name, move |b| {
            let rt = Runtime::new().unwrap();
            b.to_async(&rt).iter_custom(|iters| async move {
                let mut elapsed_time = Duration::default();
                for _ in 0..iters {
                    let shared_count = SharedMutex::new(Count::default());
                    let state = State::new(None, n as u32);
                    let shared_state = SharedMutex::new(state);
                    let (import, _tx, mut shutdown) = create_import(
                        shared_count.clone(),
                        shared_state,
                        durations,
                        buffer_size,
                        buffer_size,
                    );
                    import.notify_one();
                    let start = std::time::Instant::now();
                    black_box(
                        import_version_switch(import, version, &mut shutdown).await,
                    );
                    elapsed_time += start.elapsed();
                    shared_count.apply(|count| assert_eq!(count.transactions, n + 1));
                    // shared_count.apply(|count| println!("COUNTS: {:?}", count));
                }
                elapsed_time
            })
        });
    };

    let mut group = c.benchmark_group("import");

    let durations = Durations {
        headers: Duration::from_millis(5),
        consensus: Duration::from_millis(5),
        transactions: Duration::from_millis(5),
        executes: Duration::from_millis(10),
    };
    let n = 50usize;

    // V1
    // bench_import(&mut group, Version::V1, n, durations, 5);
    // bench_import(&mut group, Version::V1, n, durations, 10);
    bench_import(&mut group, Version::V1, n, durations, 50);

    // V2
    // bench_import(&mut group, Version::V2, n, durations, 5);
    // bench_import(&mut group, Version::V2, n, durations, 10);
    bench_import(&mut group, Version::V2, n, durations, 50);

    // V3
    // bench_import(&mut group, Version::V3, n, durations, 5);
    // bench_import(&mut group, Version::V3, n, durations, 10);
    bench_import(&mut group, Version::V3, n, durations, 50);

    // V4
    bench_import(&mut group, Version::V4, n, durations, 50);
}

criterion_group!(benches, bench_imports);
criterion_main!(benches);
