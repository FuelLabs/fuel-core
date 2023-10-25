use criterion::{
    criterion_group,
    criterion_main,
    Criterion,
};
use fuel_core_types::fuel_tx::Contract;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::time::Duration;

// 17 mb contract size
const MAX_CONTRACT_SIZE: usize = 17 * 1024 * 1024;

fn random_bytes<const N: usize, R: Rng + ?Sized>(rng: &mut R) -> Box<[u8; N]> {
    let mut bytes = Box::new([0u8; N]);
    for chunk in bytes.chunks_mut(32) {
        rng.fill(chunk);
    }
    bytes
}

fn contract_root_from_code(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(8586);

    let mut group = c.benchmark_group("contract_root");

    group.bench_function("root-from-set", |b| {
        b.iter_custom(|iters| {
            let mut elapsed_time = Duration::default();
            for _ in 0..iters {
                let bytes = random_bytes::<MAX_CONTRACT_SIZE, _>(rng);
                let contract = Contract::from(bytes.as_slice());
                let start = std::time::Instant::now();
                contract.root();
                elapsed_time += start.elapsed();
            }
            elapsed_time
        })
    });

    group.finish();
}

criterion_group!(benches, contract_root_from_code);
criterion_main!(benches);
