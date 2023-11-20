use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    Criterion,
    Throughput,
};
use digest::Digest;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use sha2::Sha256;
use std::iter::successors;

fn random_bytes32<R>(rng: &mut R) -> [u8; 32]
where
    R: Rng + ?Sized,
{
    let mut bytes = [0u8; 32];
    rng.fill(bytes.as_mut());
    bytes
}

fn hash(bytes: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(bytes);
    hash.finalize().into()
}

const UNIT_COEF: f64 = 100.0;

fn busy<R>(coefficient: f64, rng: &mut R)
where
    R: Rng + ?Sized,
{
    // Base iterations
    let iterations = coefficient as usize;
    for _ in 0..iterations {
        let bytes = random_bytes32(rng);
        black_box(hash(&bytes));
    }
}

fn generate_baseline(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(8586);
    let mut group = c.benchmark_group("baseline");
    group.bench_function("baseline", |b| b.iter(|| busy(UNIT_COEF, &mut rng)));
    group.finish();
}

fn generate_linear_data(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(8586);

    // f(x) = x
    let mut function = |n| {
        for _ in 0..n {
            busy(UNIT_COEF, &mut rng);
        }
    };

    let mut group = c.benchmark_group("linear_data");
    let independent = successors(Some(0u64), |n| Some(n + 5)).take(40);
    for n in independent {
        group.throughput(Throughput::Elements(n));
        let name = format!("linear_data_{}", n);
        group.bench_function(name, |b| b.iter(|| function(n)));
    }
    group.finish();
}

fn generate_light_linear_data(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(8586);

    // f(x) = (1/5)x + 5
    let mut function = |n| {
        for _ in 0..n {
            busy(0.2 * UNIT_COEF, &mut rng);
        }
        busy(5.0 * UNIT_COEF, &mut rng);
    };
    let mut group = c.benchmark_group("light_linear_data");
    let independent = successors(Some(0u64), |n| Some(n + 5)).take(40);
    for n in independent {
        group.throughput(Throughput::Elements(n));
        let name = format!("light_linear_data_{}", n);
        group.bench_function(name, |b| b.iter(|| function(n)));
    }
    group.finish();
}

fn generate_heavy_linear_data(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(8586);

    // f(x) = 2x + 10
    let mut function = |n| {
        for _ in 0..n {
            busy(2.0 * UNIT_COEF, &mut rng);
        }
        busy(10.0 * UNIT_COEF, &mut rng);
    };
    let mut group = c.benchmark_group("heavy_linear_data");
    let independent = successors(Some(0u64), |n| Some(n + 5)).take(40);
    for n in independent {
        group.throughput(Throughput::Elements(n));
        let name = format!("heavy_linear_data_{}", n);
        group.bench_function(name, |b| b.iter(|| function(n)));
    }
    group.finish();
}

criterion_group!(
    benches,
    generate_baseline,
    generate_linear_data,
    generate_light_linear_data,
    generate_heavy_linear_data,
);
criterion_main!(benches);
