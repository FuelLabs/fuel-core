use criterion::{
    criterion_group,
    criterion_main,
    Criterion,
};
use criterion::async_executor::FuturesExecutor;
use fuel_core_gas_price_service::v1::algorithm::SharedV1Algorithm;
use fuel_gas_price_algorithm::v1::AlgorithmV1;

#[inline]
fn dummy_algorithm() -> AlgorithmV1 {
    AlgorithmV1::default()
}

fn bench_shared_v1_algorithm(c: &mut Criterion) {
    // bench initialization of SharedV1Algorithm
    c.bench_function("SharedV1Algorithm::new_with_algorithm", |b| {
        b.iter(|| {
            let _ = SharedV1Algorithm::new_with_algorithm(dummy_algorithm());
        })
    });

    // bench writes to SharedV1Algorithm
    c.bench_function("SharedV1Algorithm::update", |b| {
        let shared_v1_algorithm =
            SharedV1Algorithm::new_with_algorithm(dummy_algorithm());

        b.to_async(FuturesExecutor).iter(|| {
            let mut value = shared_v1_algorithm.clone();
            async move {
                value.update(dummy_algorithm()).await;
            }
        });
    });

    // bench reads from SharedV1Algorithm
    c.bench_function("SharedV1Algorithm::next_gas_price", |b| {
        let shared_v1_algorithm =
            SharedV1Algorithm::new_with_algorithm(dummy_algorithm());

        b.iter(|| {
            let _ = shared_v1_algorithm.next_gas_price();
        })
    });

    // bench concurrent reads and writes to SharedV1Algorithm
    const READER_THREADS: usize = 4;
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("SharedV1Algorithm::concurrent_rw", |b| {
        let shared_v1_algorithm =
            SharedV1Algorithm::new_with_algorithm(dummy_algorithm());
        b.iter_custom(|iters| {
            let read_lock = shared_v1_algorithm.clone();
            let mut write_lock = shared_v1_algorithm.clone();
            let start = std::time::Instant::now();

            // Simulate parallel reads and writes
            rayon::scope(|s| {
                // Writer thread
                s.spawn(|_| {
                    for _ in 0..iters {
                        rt.block_on(write_lock.update(dummy_algorithm()));
                    }
                });

                // Reader threads
                for _ in 0..READER_THREADS {
                    let read_lock = read_lock.clone();
                    s.spawn(move |_| {
                        for _ in 0..(iters / READER_THREADS as u64) {
                            let _ = read_lock.next_gas_price();
                        }
                    });
                }
            });

            start.elapsed()
        });
    });
}

criterion_group!(benches, bench_shared_v1_algorithm);
criterion_main!(benches);